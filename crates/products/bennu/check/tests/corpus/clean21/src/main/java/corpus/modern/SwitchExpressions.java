package corpus.modern;

import java.time.DayOfWeek;
import java.util.List;
import java.util.Locale;

/**
 * Switch expressions (arrows, blocks with {@code yield}, colon groups with {@code yield}), enhanced
 * switches on strings and enums, pattern switches with {@code case null} and guards, and an exhaustive
 * pattern switch statement whose every arm returns.
 */
public final class SwitchExpressions {

    private SwitchExpressions() {
    }

    public enum Level {
        TRACE, DEBUG, INFO, WARN, ERROR
    }

    sealed interface Event {
    }

    record Click(int x, int y) implements Event {
    }

    record Key(char code, boolean shift) implements Event {
    }

    public static int severity(Level level) {
        return switch (level) {
            case TRACE, DEBUG -> 0;
            case INFO -> 1;
            case WARN -> {
                int base = 2;
                yield base + level.ordinal() - Level.WARN.ordinal();
            }
            case ERROR -> 3;
        };
    }

    /** Qualified enum constants as case labels (Java 21). */
    public static boolean noisy(Level level) {
        return switch (level) {
            case Level.TRACE, Level.DEBUG -> true;
            case INFO, WARN, ERROR -> false;
        };
    }

    public static String dayKind(DayOfWeek day) {
        String kind = switch (day) {
            case SATURDAY, SUNDAY -> "weekend";
            case FRIDAY -> {
                String prefix = "almost";
                yield prefix + " weekend";
            }
            default -> "weekday";
        };
        return kind.toUpperCase(Locale.ROOT);
    }

    public static int unitMillis(String unit) {
        return switch (unit.toLowerCase(Locale.ROOT)) {
            case "ms", "millis" -> 1;
            case "s", "sec", "seconds" -> 1_000;
            case "m", "min" -> 60_000;
            default -> {
                if (unit.isBlank()) {
                    yield 0;
                }
                throw new IllegalArgumentException("unknown unit " + unit);
            }
        };
    }

    public static String statusText(int code) {
        return switch (code) {
            case 200:
            case 204:
                yield "ok";
            case 404:
                yield "missing";
            default:
                String text = "status " + code;
                yield text;
        };
    }

    public static String describe(Object value) {
        return switch (value) {
            case null -> "null";
            case Integer i when i > 100 -> "big int " + i;
            case Integer i -> "int " + i;
            case String s when s.isEmpty() -> "empty string";
            case String s -> "string of " + s.length();
            case int[] array -> "ints x" + array.length;
            case List<?> list -> "list of " + list.size();
            case Level level -> "level " + level.name().toLowerCase(Locale.ROOT);
            default -> value.getClass().getSimpleName();
        };
    }

    public static int keywordScore(List<String> tokens) {
        var score = 0;
        for (var token : tokens) {
            switch (token) {
                case "class", "interface", "enum", "record" -> score += 2;
                case "var" -> score++;
                default -> {
                }
            }
        }
        return score;
    }

    /** A blank final definitely assigned by every arm of an arrow switch statement. */
    public static String badge(Level level) {
        final String badge;
        switch (level) {
            case ERROR -> badge = "[!]";
            case WARN -> badge = "[?]";
            default -> badge = "[ ]";
        }
        return badge;
    }

    /** No trailing return: the switch is exhaustive and no arm completes normally. */
    static String handle(Event event) {
        switch (event) {
            case Click(int x, int y) when x < 0 || y < 0 -> {
                return "offscreen";
            }
            case Click click -> {
                return "click at " + click.x() + "," + click.y();
            }
            case Key(char code, boolean shift) -> {
                return (shift ? "shift+" : "") + code;
            }
        }
    }

    public static String demo() {
        return severity(Level.WARN) + dayKind(DayOfWeek.FRIDAY) + unitMillis("sec") + statusText(204)
                + describe(List.of(1)) + keywordScore(List.of("var", "record")) + badge(Level.ERROR)
                + handle(new Click(1, 2)) + handle(new Key('k', true)) + noisy(Level.DEBUG);
    }
}
