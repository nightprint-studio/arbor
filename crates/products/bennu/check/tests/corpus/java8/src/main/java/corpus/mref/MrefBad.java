package corpus.mref;

import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.Supplier;

/** Method references that name nothing, the wrong arity, the wrong kind, or two things. Twin: {@link MrefOk}. */
public class MrefBad {
    static void apply(Function<Integer, String> mapper) {
    }

    void unknownJdkMethod() {
        Function<String, Integer> mapper = String::nope; // error: compiler.err.invalid.mref
    }

    void unknownProjectMethod() {
        Function<String, Integer> mapper = MrefTypes.Named::nope; // error: compiler.err.invalid.mref
    }

    void targetHasOneParameterTooMany() {
        BiFunction<String, String, Integer> mapper = String::length; // error: compiler.err.prob.found.req
    }

    void targetHasNoReceiverToBind() {
        Supplier<Integer> supplier = String::length; // error: compiler.err.prob.found.req
    }

    void instanceMethodThroughTheTypeWithoutAReceiver() {
        Function<String, Integer> mapper = MrefTypes.Named::instanceLength; // error: compiler.err.prob.found.req
    }

    void staticMethodThroughAnInstance(MrefTypes.Named named) {
        Function<String, Integer> mapper = named::staticLength; // error: compiler.err.prob.found.req
    }

    void constructorWithoutAMatchingOverload() {
        Function<Integer, MrefTypes.Named> factory = MrefTypes.Named::new; // error: compiler.err.prob.found.req
    }

    void staticAndInstanceOverloadsBothMatch() {
        Function<Integer, String> mapper = Integer::toString; // error: compiler.err.prob.found.req
    }

    void ambiguousReferenceAsAnArgument() {
        apply(Integer::toString); // error: compiler.err.cant.apply.symbol
    }

    void incompatibleReturnType() {
        Function<String, String> mapper = String::length; // error: compiler.err.prob.found.req
    }

    void noFunctionalTarget() {
        Object target = String::length; // error: compiler.err.prob.found.req
    }
}
