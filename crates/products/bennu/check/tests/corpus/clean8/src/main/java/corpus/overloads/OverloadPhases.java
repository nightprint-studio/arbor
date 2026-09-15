package corpus.overloads;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

/**
 * Overload resolution through javac's three phases: strict (identity + widening), loose (boxing and
 * unboxing), varargs. Each call below picks exactly one candidate; the comment names it.
 */
public class OverloadPhases {

    static String take(long value) {
        return "long:" + value;
    }

    static String take(Integer value) {
        return "Integer:" + value;
    }

    static String take(Object value) {
        return "Object:" + value;
    }

    static String take(int... values) {
        return "int...:" + values.length;
    }

    static String pick(String text) {
        return "String:" + text;
    }

    static String pick(Object any) {
        return "Object:" + any;
    }

    static String join(String first, String second) {
        return first + second;
    }

    static String join(String... parts) {
        StringBuilder out = new StringBuilder();
        for (String part : parts) {
            out.append(part);
        }
        return out.toString();
    }

    static <T> String generic(T value) {
        return "T:" + value;
    }

    static String generic(String value) {
        return "String:" + value;
    }

    static long sum(long a, long b) {
        return a + b;
    }

    static double sum(double... values) {
        double total = 0;
        for (double value : values) {
            total += value;
        }
        return total;
    }

    static String unbox(int value) {
        return "int:" + value;
    }

    static String unbox(Object value) {
        return "Object:" + value;
    }

    /** Every call resolves; the chosen overload is named in the trailing comment. */
    public static List<String> resolveAll() {
        List<String> out = new ArrayList<String>();
        out.add(take(5));                     // long: widening beats boxing
        out.add(take('c'));                   // long: char widens to long
        out.add(take(5L));                    // long: identity
        out.add(take(Integer.valueOf(5)));    // Integer: identity beats widening reference
        out.add(take("text"));                // Object
        out.add(take());                      // int...: varargs with zero arguments
        out.add(take(1, 2, 3));               // int...
        out.add(pick(null));                  // String: most specific
        out.add(join("a", "b"));              // fixed arity wins in phase 1
        out.add(join("a", "b", "c"));         // varargs
        out.add(join());                      // varargs, zero arguments
        out.add(generic("x"));                // String: more specific than T
        out.add(generic(42));                 // T, inferred Integer
        out.add(String.valueOf(sum(1, 2)));   // long, long by widening
        out.add(String.valueOf(sum(1.5, 2))); // double... : no fixed-arity candidate applies
        out.add(unbox(Integer.valueOf(7)));   // Object: phase 1 never unboxes
        out.add(unbox(7));                    // int
        out.add(String.format("plain"));      // format(String, Object...) with zero varargs
        return out;
    }

    /** Boxing in the loose phase, inside a generic collection API. */
    public static int boxedTotal() {
        List<Integer> numbers = Arrays.asList(3, 1, 2);
        Collections.sort(numbers);
        int total = 0;
        for (int number : numbers) {
            total += number;
        }
        Integer boxed = total;
        long widened = boxed;
        return (int) widened + numbers.get(0);
    }

    /** Overloads inherited and added in a subclass are one overload set. */
    public static class Printer {
        public String print(Object value) {
            return "object " + value;
        }

        public String print(Number value) {
            return "number " + value;
        }
    }

    public static class CharPrinter extends Printer {
        public String print(char value) {
            return "char " + value;
        }

        @Override
        public String print(Number value) {
            return "overridden " + super.print(value);
        }

        public String printAll() {
            return print('x') + print(3) + print((Object) "s") + print(2.5f);
        }
    }
}
