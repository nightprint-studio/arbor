package corpus.lambda;

import java.util.function.Function;
import java.util.function.Supplier;

/** Lambdas and method references that fit no target, or fit two. Twin: {@link LambdaOk}. */
public class LambdaBad {
    void parameterForRunnable() {
        LambdaTargets.run(x -> { }); // error: compiler.err.cant.apply.symbol
    }

    void valueBodyForRunnable() {
        LambdaTargets.run(() -> 42); // error: compiler.err.cant.apply.symbol
    }

    void twoParametersForFunction() {
        LambdaTargets.mapString((a, b) -> 1); // error: compiler.err.cant.apply.symbol
    }

    void oneParameterForBiConsumer() {
        LambdaTargets.consumeTwo(s -> { }); // error: compiler.err.cant.apply.symbol
    }

    void intBodyForSupplierOfString() {
        LambdaTargets.supply(() -> 1); // error: compiler.err.cant.apply.symbol
    }

    void intBodyForPredicate() {
        LambdaTargets.predicate(s -> s.length()); // error: compiler.err.cant.apply.symbol
    }

    void implicitLambdaFitsTwoOverloads() {
        LambdaTargets.overloadedMapper(s -> s.length()); // error: compiler.err.ref.ambiguous
    }

    void unknownMethodReference() {
        LambdaTargets.mapString(String::nope); // error: compiler.err.invalid.mref
    }

    void methodReferenceReturnsTheWrongType() {
        LambdaTargets.mapString(String::isEmpty); // error: compiler.err.cant.apply.symbol
    }

    void unboundReceiverWithNothingToBindIt() {
        LambdaTargets.supply(String::length); // error: compiler.err.cant.apply.symbol
    }

    void lambdaArityInAnAssignment() {
        Runnable task = x -> { }; // error: compiler.err.prob.found.req
    }

    void lambdaReturnTypeInAnAssignment() {
        Function<String, Integer> mapper = s -> s; // error: compiler.err.prob.found.req
    }

    void unknownMemberOnALambdaParameter() {
        Function<String, Integer> mapper = s -> s.nope(); // error: compiler.err.cant.resolve.location.args
    }

    void unknownMethodReferenceInAnAssignment() {
        Supplier<String> supplier = String::nope; // error: compiler.err.invalid.mref
    }

    void lambdaWithANonFunctionalTarget() {
        Object target = () -> { }; // error: compiler.err.prob.found.req
    }

    void valueReturnedFromAVoidLambda() {
        Runnable task = () -> { return 1; }; // error: compiler.err.prob.found.req
    }
}
