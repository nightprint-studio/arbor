package corpus.annotation;

import java.util.List;

/** Annotation type declarations with illegal members. Twin: {@link AnnotationDeclOk}. */
public class AnnotationDeclBad {
    static String mutable = "x";

    @interface ObjectMember {
        Object value(); // error: compiler.err.invalid.annotation.member.type
    }

    @interface GenericCollectionMember {
        List<String> values(); // error: compiler.err.invalid.annotation.member.type
    }

    @interface ArbitraryClassMember {
        Thread thread(); // error: compiler.err.invalid.annotation.member.type
    }

    @interface TwoDimensionalArrayMember {
        String[][] grid(); // error: compiler.err.invalid.annotation.member.type
    }

    @interface MemberWithParameters {
        String value(int index); // error: compiler.err.intf.annotation.members.cant.have.params
    }

    @interface GenericMethodMember {
        <T> String value(); // error: compiler.err.intf.annotation.members.cant.have.type.params
    }

    @interface MemberWithAThrowsClause {
        String value() throws Exception; // error: compiler.err.throws.not.allowed.in.intf.annotation
    }

    @interface DefaultOfTheWrongType {
        int value() default "x"; // error: compiler.err.prob.found.req
    }

    @interface NonConstantDefault {
        String value() default mutable; // error: compiler.err.attribute.value.must.be.constant
    }
}
