package corpus.resolve;

import java.util.List;

/** Type names that resolve to nothing, in every position a type can be written. Twin: {@link ResolveTypeOk}. */
public class ResolveTypeBad {
    static class Outer {
        static class Inner {
        }
    }

    UnknownFieldType field; // error: compiler.err.cant.resolve.location

    void localType() {
        Missing value = null; // error: compiler.err.cant.resolve.location
    }

    void newInstance() {
        Object value = new Missing(); // error: compiler.err.cant.resolve.location
    }

    void genericArgument(List<Missing> values) { // error: compiler.err.cant.resolve.location
    }

    void nestedTypeOfKnownOuter() {
        Outer.Absent value = null; // error: compiler.err.cant.resolve.location
    }

    void qualifiedTypeInJdkPackage() {
        java.util.Absent value = null; // error: compiler.err.cant.resolve.location
    }

    void qualifiedTypeInMissingPackage() {
        com.nowhere.Thing value = null; // error: compiler.err.doesnt.exist
    }

    void castTarget(Object value) {
        Object result = (Missing) value; // error: compiler.err.cant.resolve.location
    }

    void instanceofTarget(Object value) {
        boolean result = value instanceof Missing; // error: compiler.err.cant.resolve.location
    }

    void catchParameter() {
        try {
            System.gc();
        } catch (MissingException ex) { // error: compiler.err.cant.resolve.location
        }
    }

    Missing returnType() { // error: compiler.err.cant.resolve.location
        return null;
    }

    void arrayCreation() {
        Object result = new Missing[3]; // error: compiler.err.cant.resolve.location
    }

    void classLiteral() {
        Object result = Missing.class; // error: compiler.err.cant.resolve.location
    }

    <T extends Missing> void typeParameterBound() { // error: compiler.err.cant.resolve.location
    }

    void throwsClause() throws MissingException { // error: compiler.err.cant.resolve.location
    }
}
