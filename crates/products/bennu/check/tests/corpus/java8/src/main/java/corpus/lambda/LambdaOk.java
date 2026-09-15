package corpus.lambda;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.IntFunction;
import java.util.function.Supplier;
import java.util.stream.Collectors;

/** Legal twins of {@link LambdaBad}, plus overloads a lambda or method reference legally selects. */
public class LambdaOk {
    static int compute() {
        return 1;
    }

    static <T, R> R apply(T value, Function<T, R> mapper) {
        return mapper.apply(value);
    }

    void voidBodyForRunnable() {
        LambdaTargets.run(() -> { });
    }

    void statementExpressionBodyForRunnable() {
        LambdaTargets.run(() -> System.out.println());
    }

    void methodReferenceForRunnable(List<String> names) {
        LambdaTargets.run(names::clear);
    }

    void valueBodyForCallable() {
        String result = LambdaTargets.call(() -> "x");
    }

    void voidBodyPicksRunnable() {
        LambdaTargets.submit(() -> { });
    }

    void valueBodyPicksCallable() {
        LambdaTargets.submit(() -> "x");
    }

    void callWithAResultPrefersCallable() {
        LambdaTargets.submit(() -> compute());
    }

    void throwingBodyPrefersCallable() {
        LambdaTargets.submit(() -> { throw new IllegalStateException(); });
    }

    void oneParameterForFunction() {
        LambdaTargets.mapString(s -> s.length());
    }

    void twoParametersForBiConsumer() {
        LambdaTargets.consumeTwo((s, i) -> { });
    }

    void stringForSupplier() {
        LambdaTargets.supply(() -> "x");
        LambdaTargets.supply(String::new);
    }

    void booleanForPredicate() {
        LambdaTargets.predicate(s -> s.isEmpty());
        LambdaTargets.predicate(String::isEmpty);
    }

    void exactMethodReferencePicksThePrimitiveOverload() {
        LambdaTargets.overloadedMapper(String::length);
    }

    void explicitLambdaPicksThePrimitiveOverload() {
        LambdaTargets.overloadedMapper((String s) -> s.length());
    }

    void methodReferencesForFunction() {
        LambdaTargets.mapString(String::length);
        LambdaTargets.mapString(Integer::parseInt);
    }

    void lambdasInAssignments() {
        Runnable task = () -> { };
        Function<String, Integer> mapper = s -> s.length();
        Callable<Integer> callable = () -> 1;
    }

    void methodReferenceKinds() {
        Supplier<String> bound = "x"::trim;
        Function<String, String> unbound = String::toUpperCase;
        BiFunction<Integer, Integer, Integer> staticReference = Integer::sum;
        IntFunction<int[]> arrayConstructor = int[]::new;
        Supplier<ArrayList<String>> constructor = ArrayList::new;
    }

    void jdkFunctionalApis(List<String> names) {
        Comparator<String> byLength = Comparator.comparing(String::length);
        names.forEach(System.out::println);
        List<Integer> lengths = names.stream().map(String::length).collect(Collectors.toList());
        names.sort((left, right) -> left.compareTo(right));
    }

    void lambdaReturningADiamond() {
        Map<String, List<String>> groups = new HashMap<>();
        groups.computeIfAbsent("key", key -> new ArrayList<>()).add("value");
    }

    void capturesAnEffectivelyFinalLocal() {
        String prefix = "p";
        Function<String, String> prepend = value -> prefix + value;
    }

    void castGivesTheLambdaItsTarget() {
        Object task = (Runnable) () -> { };
    }

    void genericMethodInfersFromTheReference() {
        int length = apply("x", String::length);
    }

    void blockBodyReturningAValue() {
        Function<String, Integer> mapper = s -> {
            return s.length();
        };
    }
}
