package corpus.version;

/** Everything Java 15 allows: switch rules and expressions, `yield`, multiple labels, text blocks. */
public class Gate15Ok {
    enum Color { RED, GREEN }

    String describeADay(int day) {
        return switch (day) {
            case 1, 7 -> "weekend";
            case 2, 3, 4, 5, 6 -> {
                String label = "week";
                yield label + "day";
            }
            default -> throw new IllegalArgumentException("day " + day);
        };
    }

    int yieldInTheColonForm(String key) {
        return switch (key) {
            case "a":
                yield 1;
            case "b":
                yield 2;
            default:
                yield 0;
        };
    }

    int anEnumSwitchExpressionNeedsNoDefault(Color color) {
        return switch (color) {
            case RED -> 1;
            case GREEN -> 2;
        };
    }

    void ruleStatements(int day) {
        switch (day) {
            case 1 -> System.gc();
            case 2 -> {
                System.gc();
            }
            default -> throw new IllegalStateException();
        }
    }

    String textBlock() {
        String block = """
            hello \
            world\s
            """;
        return block.formatted();
    }

    void restrictedIdentifiersAreStillPlainNames() {
        int record = 1;
        String sealed = "sealed";
        String permits = "permits";
        System.out.println(record + sealed + permits);
    }

    boolean instanceofWithoutABinding(Object value) {
        return value instanceof String;
    }
}
