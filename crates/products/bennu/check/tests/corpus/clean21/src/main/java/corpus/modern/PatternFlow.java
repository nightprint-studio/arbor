package corpus.modern;

import java.util.Iterator;
import java.util.List;
import java.util.Optional;

/**
 * {@code instanceof} patterns and the scope of their bindings: out of a negated {@code if}, into the
 * right operand of {@code &&} / {@code ||}, out of a {@code while} that only exits on a match, into an
 * {@code else}, and shadowing a field.
 */
public class PatternFlow {

    private final String text;

    public PatternFlow(String text) {
        this.text = text;
    }

    public static int lengthOf(Object value) {
        if (!(value instanceof String text)) {
            return -1;
        }
        return text.length();
    }

    public static String firstWord(Object value) {
        if (!(value instanceof CharSequence sequence) || sequence.isEmpty()) {
            return "";
        }
        var trimmed = sequence.toString().strip();
        var space = trimmed.indexOf(' ');
        return space < 0 ? trimmed : trimmed.substring(0, space);
    }

    record Circle(double radius) {
    }

    record Square(double side) {
    }

    record Named(String name) {
    }

    public static double area(Object shape) {
        if (shape instanceof Circle circle && circle.radius() > 0) {
            return Math.PI * circle.radius() * circle.radius();
        } else if (shape instanceof Square(double side)) {
            return side * side;
        }
        return 0;
    }

    public static int sumNumbers(List<?> values) {
        var total = 0;
        for (Object value : values) {
            if (value instanceof Integer number) {
                total += number;
            } else if (value instanceof String digits && !digits.isBlank()) {
                total += Integer.parseInt(digits.strip());
            }
        }
        return total;
    }

    /** The binding is in scope after the loop: the loop only exits once the pattern matched. */
    public static String nextString(Iterator<?> items) {
        Object current = items.hasNext() ? items.next() : "";
        while (!(current instanceof String found)) {
            current = items.hasNext() ? items.next() : "";
        }
        return found;
    }

    public String shadowed(Object other) {
        if (other instanceof String text) {
            return text + "/" + this.text;
        }
        return text;
    }

    public static String unwrap(Object value) {
        if (value instanceof Optional<?> optional && optional.isPresent() && optional.get() instanceof String inner) {
            return inner;
        }
        return String.valueOf(value);
    }

    public static boolean sameName(Object left, Object right) {
        return left instanceof Named(String a) && right instanceof Named(String b) && a.equalsIgnoreCase(b);
    }

    public static String kind(Object value) {
        if (!(value instanceof Number number)) {
            return "not a number";
        } else {
            return number instanceof Integer ? "int" : "number " + number.doubleValue();
        }
    }

    public static int size(Object value) {
        return value instanceof String s ? s.length() : value instanceof List<?> list ? list.size() : 0;
    }

    public String demo() {
        return lengthOf("abc") + firstWord("hello world") + area(new Square(2)) + sumNumbers(List.of(1, "2"))
                + nextString(List.of(1, "two").iterator()) + shadowed("x") + unwrap(Optional.of("in"))
                + sameName(new Named("A"), new Named("a")) + kind(2.5) + size(List.of());
    }
}
