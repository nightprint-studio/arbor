package corpus.infer;

import corpus.infer.InferTypes.Circle;
import corpus.infer.InferTypes.Color;
import corpus.infer.InferTypes.Point;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/** Java 10-21 constructs that give a variable its type: `var`, patterns, switch expressions, enhanced for. */
public class InferModern {
    void local_variables(Map<String, Integer> counts) {
        var list = new ArrayList<String>();
        String first = list.get(0); // infer
        var table = Map.of("a", 1);
        Integer looked = table.get("a"); // infer
        var length = "abc".length();
        long widened = length + 1L; // infer
        for (var entry : counts.entrySet()) {
            String key = entry.getKey(); // infer
            Integer value = entry.getValue(); // infer
        }
        for (String name : list) {
            int size = name.length(); // infer
        }
    }

    void patterns(Object any) {
        if (any instanceof String text) {
            int length = text.length(); // infer
            String upper = text.toUpperCase(); // infer
        }
        if (any instanceof Point(int x, int y)) {
            int sum = x + y; // infer
        }
        if (any instanceof List<?> items && !items.isEmpty()) {
            int size = items.size(); // infer
        }
        if (!(any instanceof Integer number)) {
            return;
        }
        long wide = number + 1L; // infer
    }

    void switches(int code, Color color) {
        int fromInt = switch (code) { case 1 -> 10; default -> 20; }; // infer
        String fromEnum = switch (color) { case RED -> "r"; default -> "o"; }; // infer
        long mixed = switch (code) { case 1 -> 1; default -> 2L; }; // infer
    }

    String sealedSwitch(InferTypes.Shape shape) {
        String described = switch (shape) { // infer
            case Circle c -> c.describe();
            default -> "other";
        };
        return described;
    }

    void lambdas(List<String> names) {
        names.forEach(name -> {
            int length = name.length(); // infer
        });
        names.stream().map(name -> name.trim()).forEach(trimmed -> {
            String again = trimmed.trim(); // infer
        });
    }
}
