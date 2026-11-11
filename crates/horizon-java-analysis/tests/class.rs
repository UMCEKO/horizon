//! The class reader against classes javac compiles from `tests/java/`.

mod common;

use common::classes;
use horizon_java_analysis::class::{self, Const, Event, Parse, Value};
use serde_json::{Value as Json, json};

fn load(name: &str) -> Option<Vec<u8>> {
    classes().map(|dir| std::fs::read(dir.join(name)).unwrap())
}

#[test]
fn one_extends_a_vanilla_class_and_keeps_declaration_order() {
    let Some(data) = load("fixture/One.class") else {
        return;
    };
    let info = class::parse_class(&data, Parse::all()).unwrap();
    assert_eq!(info.name, "fixture/One");
    assert_eq!(info.sup.as_deref(), Some("net/minecraft/world/Thing"));
    let methods: Vec<(&str, &str)> = info
        .methods
        .iter()
        .map(|m| (m.name.as_str(), m.desc.as_str()))
        .collect();
    for wanted in [
        ("tick", "()V"),
        ("tick", "(J)V"),
        ("must", "()V"),
        ("friction", "()I"),
        ("own", "()V"),
        ("<init>", "()V"),
    ] {
        assert!(methods.contains(&wanted), "{wanted:?} in {methods:?}");
    }
    let position =
        |name: &str, desc: &str| methods.iter().position(|m| *m == (name, desc)).unwrap();
    assert!(position("tick", "()V") < position("tick", "(J)V"));
    assert!(position("tick", "(J)V") < position("must", "()V"));
    assert!(position("must", "()V") < position("friction", "()I"));
    assert!(position("friction", "()I") < position("own", "()V"));
}

#[test]
fn listeners_constructor_registers_through_lambdas() {
    let Some(data) = load("fixture/Listeners.class") else {
        return;
    };
    let info = class::parse_class(&data, Parse::all()).unwrap();
    let init = info.methods.iter().find(|m| m.name == "<init>").unwrap();
    let indy = init
        .events
        .iter()
        .position(|e| matches!(e, Event::Indy { name, desc, .. } if name == "accept" && desc == "(Lfixture/Listeners;)Ljava/util/function/Consumer;"))
        .expect("the method reference to onCancel");
    let call = init
        .events
        .iter()
        .position(|e| matches!(e, Event::Call { owner, name, .. } if owner == "net/neoforged/bus/api/IEventBus" && name == "addListener"))
        .expect("addListener");
    assert!(indy < call);
    assert!(init.events.contains(&Event::Static {
        owner: "net/neoforged/bus/api/EventPriority".into(),
        name: "LOW".into()
    }));
    let calls = init
        .events
        .iter()
        .filter(|e| matches!(e, Event::Call { name, .. } if name == "addListener"))
        .count();
    assert_eq!(calls, 3);
    let on = info.methods.iter().find(|m| m.name == "onPlain").unwrap();
    let subscribe = on
        .annos
        .iter()
        .find(|a| a.desc == "Lnet/neoforged/bus/api/SubscribeEvent;")
        .unwrap();
    let values: Vec<(&str, Json)> = subscribe
        .values
        .iter()
        .map(|(k, v)| (k.as_str(), class::python_value(v)))
        .collect();
    assert_eq!(
        values,
        [("priority", json!("HIGH")), ("receiveCanceled", json!(1))]
    );
}

#[test]
fn annotations_keep_what_python_drops() {
    let Some(data) = load("fixture/Annos.class") else {
        return;
    };
    let info = class::parse_class(&data, Parse::all()).unwrap();
    assert_eq!(info.invisible.len(), 1);
    assert_eq!(info.invisible[0].desc, "Lfixture/Inv;");
    assert_eq!(
        info.invisible[0].values,
        [(
            "value".to_string(),
            Value::Array(vec![Value::Str("a".into()), Value::Str("b".into())])
        )]
    );
    assert_eq!(info.annos.len(), 1);
    let vis = &info.annos[0];
    let get = |name: &str| &vis.values.iter().find(|(k, _)| k == name).unwrap().1;
    assert_eq!(
        get("e"),
        &Value::Enum {
            desc: "Ljava/lang/annotation/ElementType;".into(),
            name: "FIELD".into()
        }
    );
    assert_eq!(get("c"), &Value::Class("Ljava/lang/String;".into()));
    assert_eq!(
        get("z"),
        &Value::Const {
            tag: 'Z',
            value: Const::Int(1)
        }
    );
    assert_eq!(
        get("i"),
        &Value::Const {
            tag: 'I',
            value: Const::Int(7)
        }
    );
    assert_eq!(class::python_value(get("z")), json!(1));
    assert_eq!(class::python_value(get("i")), Json::Null);
    assert_eq!(class::python_value(get("e")), json!("FIELD"));
    assert_eq!(
        class::python_annotations(&info.annos)["Lfixture/Vis;"]["c"],
        json!("Ljava/lang/String;")
    );
}

#[test]
fn a_truncated_class_is_an_error() {
    let Some(data) = load("fixture/One.class") else {
        return;
    };
    assert!(class::parse_class(&data[..20], Parse::all()).is_err());
    for cut in 0..data.len() {
        let _ = class::parse_class(&data[..cut], Parse::all());
    }
}

#[test]
fn the_header_alone_needs_no_members() {
    let Some(data) = load("fixture/One.class") else {
        return;
    };
    let info = class::parse_class(
        &data,
        Parse {
            members: false,
            annotations: false,
            code: false,
        },
    )
    .unwrap();
    assert!(info.methods.is_empty());
    assert_eq!(info.sup.as_deref(), Some("net/minecraft/world/Thing"));
}

#[test]
fn descriptors_split_into_parameter_types() {
    assert_eq!(
        class::params("(ILjava/lang/String;[[JLa/B;)V"),
        ["I", "java/lang/String", "[[J", "a/B"]
    );
    assert_eq!(class::params("()V"), Vec::<String>::new());
}

#[test]
fn a_tableswitch_is_padded_to_four_bytes() {
    let table = |pad: usize| {
        let mut code = vec![0u8; 0];
        code.push(0xaa);
        code.extend(vec![0; pad]);
        code.extend(0i32.to_be_bytes());
        code.extend(0i32.to_be_bytes());
        code.extend(2i32.to_be_bytes());
        code.extend(vec![0; 12]);
        code
    };
    assert_eq!(class::op_length(&table(3), 0).unwrap(), 28);
    let mut shifted = vec![0u8];
    shifted.extend(table(2));
    assert_eq!(class::op_length(&shifted, 1).unwrap(), 27);
    assert!(class::op_length(&table(3)[..10], 0).is_err());
}

#[test]
fn a_lookupswitch_counts_its_pairs() {
    let mut code = vec![0xab, 0, 0, 0];
    code.extend(0i32.to_be_bytes());
    code.extend(2i32.to_be_bytes());
    code.extend(vec![0; 16]);
    assert_eq!(class::op_length(&code, 0).unwrap(), 4 + 8 + 16);
}

#[test]
fn classes_are_vanilla_or_extensions_by_name() {
    assert!(class::is_vanilla("net/minecraft/world/Base"));
    assert!(class::is_vanilla("com/mojang/serialization/Codec"));
    assert!(!class::is_vanilla("net/neoforged/bus/api/Event"));
    assert!(class::is_extension(
        "net/neoforged/neoforge/common/extensions/IThingExtension"
    ));
    assert!(!class::is_extension(
        "net/neoforged/neoforge/common/extensions/Thing"
    ));
}
