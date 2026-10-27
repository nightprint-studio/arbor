package corpus.infer;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;

/** Calls into the JDK: strings, math overloads, collections, optionals. */
public class InferJdk {
    void strings(String name) {
        String trimmed = name.trim(); // infer
        int length = name.length(); // infer
        char first = name.charAt(0); // infer
        String[] parts = name.split(","); // infer
        boolean empty = name.isEmpty(); // infer
        String upper = name.toUpperCase(); // infer
        int compared = name.compareTo("b"); // infer
        String replaced = name.replace("a", "b"); // infer
        StringBuilder builder = new StringBuilder(); // infer
        StringBuilder appended = new StringBuilder().append("a").append(1); // infer
        String built = builder.toString(); // infer
        String valueOf = String.valueOf(1); // infer
        String formatted = String.format("%d", 1); // infer
    }

    void math() {
        int abs = Math.abs(-1); // infer
        double absDouble = Math.abs(-1.5); // infer
        long rounded = Math.round(1.5); // infer
        int roundedFloat = Math.round(1.5f); // infer
        long widest = Math.max(1, 2L); // infer
        double mixed = Math.max(1, 2.0); // infer
        double root = Math.sqrt(2); // infer
        int parsed = Integer.parseInt("1"); // infer
        Integer boxed = Integer.valueOf(1); // infer
        long fromLong = Long.parseLong("1"); // infer
    }

    void boxing(Integer boxed, Long boxedLong) {
        int unboxed = boxed + 1; // infer
        long widened = boxed + 1L; // infer
        long both = boxed + boxedLong; // infer
        boolean compared = boxed > 1; // infer
    }

    void lists(List<String> names, ArrayList<Integer> numbers) {
        String got = names.get(0); // infer
        int size = names.size(); // infer
        boolean contains = names.contains("a"); // infer
        Iterator<String> iterator = names.iterator(); // infer
        String next = iterator.next(); // infer
        List<String> sub = names.subList(0, 1); // infer
        Integer number = numbers.get(0); // infer
        List<String> fixed = Arrays.asList("a", "b"); // infer
        List<Integer> immutable = List.of(1, 2); // infer
        List<String> single = Collections.singletonList("a"); // infer
        List<String> empty = Collections.<String>emptyList(); // infer
    }

    void maps(HashMap<String, Integer> counts, Map<String, List<String>> groups) {
        Integer count = counts.get("a"); // infer
        Set<String> keys = counts.keySet(); // infer
        Collection<Integer> values = counts.values(); // infer
        Set<Map.Entry<String, Integer>> entries = counts.entrySet(); // infer
        List<String> group = groups.get("a"); // infer
        boolean has = counts.containsKey("a"); // infer
        HashMap<String, Integer> created = new HashMap<String, Integer>(); // infer
        Integer orDefault = counts.getOrDefault("a", 0); // infer
        int sum = counts.values().stream().mapToInt(Integer::intValue).sum(); // infer
    }

    void entries(Map.Entry<String, Integer> entry) {
        String key = entry.getKey(); // infer
        Integer value = entry.getValue(); // infer
    }

    void optionals() {
        Optional<String> some = Optional.of("a"); // infer
        String orElse = Optional.of("a").orElse("b"); // infer
        Optional<Integer> mapped = Optional.of("a").map(String::length); // infer
        boolean present = some.isPresent(); // infer
        String got = some.get(); // infer
    }
}
