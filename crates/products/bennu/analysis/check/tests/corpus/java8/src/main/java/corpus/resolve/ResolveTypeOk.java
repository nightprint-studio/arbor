package corpus.resolve;

import java.util.List;
import java.util.Map;

/** Legal twins of {@link ResolveTypeBad}, plus type names that resolve through less obvious routes. */
public class ResolveTypeOk {
    static class Outer {
        static class Inner {
        }

        class InnerInstance {
        }
    }

    interface Shape {
    }

    Outer.Inner field;

    void localType() {
        Outer value = null;
    }

    void newInstance() {
        Object value = new Outer();
    }

    void genericArgument(List<Outer> values) {
    }

    void nestedTypeOfKnownOuter() {
        Outer.Inner value = new Outer.Inner();
    }

    void qualifiedTypeInJdkPackage() {
        java.util.ArrayList<String> value = null;
    }

    void nestedJdkTypeThroughImportAndFullyQualified() {
        Map.Entry<String, Integer> entry = null;
        java.util.Map.Entry<String, Integer> other = entry;
    }

    void castTarget(Object value) {
        Object result = (Outer) value;
    }

    void instanceofTarget(Object value) {
        boolean result = value instanceof Outer;
    }

    void catchParameter() {
        try {
            System.gc();
        } catch (IllegalStateException ex) {
        }
    }

    Outer returnType() {
        return null;
    }

    void arrayCreation() {
        Object result = new Outer[3];
    }

    void classLiterals() {
        Object result = Outer.class;
        Object primitive = int.class;
        Object nothing = void.class;
    }

    <T extends Shape> void typeParameterBound() {
    }

    <T> void typeParameterUsedAsType(T value) {
        T copy = value;
    }

    void innerInstanceType() {
        Outer outer = new Outer();
        Outer.InnerInstance inner = outer.new InnerInstance();
    }

    void implicitJavaLang() {
        StringBuilder builder = new StringBuilder();
        Thread thread = null;
        Runnable task = null;
    }

    void localClassAsType() {
        class Local {
        }
        Local local = new Local();
    }

    void fullyQualifiedJavaLang() {
        java.lang.String text = "x";
    }

    void fullyQualifiedOwnType() {
        corpus.resolve.ResolveTypeOk self = this;
    }

    void throwsClause() throws IllegalStateException {
    }
}
