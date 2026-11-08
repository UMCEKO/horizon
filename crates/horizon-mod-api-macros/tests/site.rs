#![forbid(unsafe_code)]
extern crate self as mod_points;
pub enum SiteId {
    One,
}
pub mod __private {
    pub fn wrap<O>(
        _: super::SiteId,
        _: usize,
        call: impl FnOnce(Option<(*const (), usize)>) -> O,
    ) -> O {
        call(None)
    }
}
struct Game<'a>(&'a mut u32);
struct Args {
    value: u32,
}
type Receiver<'g, 'r> = &'r mut Game<'g>;
type Output = u32;
type Listener = for<'g> fn(
    Receiver<'g, '_>,
    &mut Args,
    &mut dyn FnMut(Receiver<'g, '_>, &mut Args) -> Output,
) -> Output;

#[test]
fn site_macro_reborrows_a_lifetime_parameterized_receiver_without_unsafe_in_the_caller() {
    let mut value = 3;
    let mut game = Game(&mut value);
    let mut args = Args { value: 2 };
    let result = horizon_mod_api_macros::site!(
        __test_site_wrap,
        One,
        Listener,
        Receiver,
        Args,
        Output,
        &mut game,
        args,
        |receiver: Receiver<'_, '_>, args: &mut Args| {
            *receiver.0 += args.value;
            *receiver.0
        }
    );
    assert_eq!(result, 5);
    assert_eq!(value, 5);
}
