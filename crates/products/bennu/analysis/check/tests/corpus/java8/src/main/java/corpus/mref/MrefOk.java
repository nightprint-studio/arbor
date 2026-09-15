package corpus.mref;

import java.util.ArrayList;
import java.util.List;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.IntBinaryOperator;
import java.util.function.Supplier;
import java.util.function.ToIntFunction;

/** Legal twins of {@link MrefBad}: every method-reference form, including overloaded targets. */
public class MrefOk {
    static void apply(Function<Integer, String> mapper) {
    }

    static void overloaded(Function<String, Integer> mapper) {
    }

    static void overloaded(Supplier<String> supplier) {
    }

    String describe() {
        return "";
    }

    void unboundReceiver() {
        Function<String, Integer> mapper = String::length;
    }

    void unboundReceiverWithAnArgument() {
        BiFunction<String, String, Boolean> equals = String::equalsIgnoreCase;
    }

    void boundReceiver(MrefTypes.Named named) {
        Function<String, Integer> mapper = named::instanceLength;
    }

    void boundReceiverOnALiteral() {
        Supplier<Integer> supplier = "text"::length;
    }

    void staticMethodThroughTheType() {
        Function<String, Integer> mapper = MrefTypes.Named::staticLength;
    }

    void constructorReference() {
        Function<String, MrefTypes.Named> factory = MrefTypes.Named::new;
    }

    void arrayConstructorReference() {
        Function<Integer, int[]> factory = int[]::new;
    }

    void genericConstructorReference() {
        Supplier<List<String>> factory = ArrayList::new;
    }

    void thisAndSuperReferences() {
        Supplier<String> own = this::describe;
        Supplier<String> inherited = super::toString;
    }

    void overloadedStaticMethodSettledByTheTarget() {
        apply(String::valueOf);
        Function<Integer, String> mapper = String::valueOf;
    }

    void overloadedTargetsSettledByArity() {
        overloaded(String::length);
        overloaded(() -> "x");
    }

    void overloadedJdkMethods() {
        IntBinaryOperator max = Math::max;
        ToIntFunction<String> parse = Integer::parseInt;
        Consumer<String> printer = System.out::println;
    }

    void objectMethodThroughAnUnboundReceiver() {
        Function<Object, String> describe = Object::toString;
    }
}
