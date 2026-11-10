package fixture;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

@Inv({"a", "b"})
@Vis(e = ElementType.FIELD, c = String.class, z = true, i = 7)
public class Annos {}

@Retention(RetentionPolicy.CLASS)
@interface Inv {
    String[] value();
}

@Retention(RetentionPolicy.RUNTIME)
@interface Vis {
    ElementType e();

    Class<?> c();

    boolean z();

    int i();
}
