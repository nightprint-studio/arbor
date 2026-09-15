package corpus.structure;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;

/**
 * Inner classes and {@code Outer.this}, local and anonymous classes capturing effectively-final
 * locals, a lambda inside an anonymous class inside a generic inner class, fields shadowed by
 * locals and parameters, initializers, a field and methods sharing a name, labels.
 */
public class Outer {

    private static final Map<String, Integer> DEFAULTS;

    static {
        Map<String, Integer> defaults = new HashMap<String, Integer>();
        defaults.put("retries", 3);
        defaults.put("timeout", 30);
        DEFAULTS = Collections.unmodifiableMap(defaults);
    }

    private final String name;
    private final List<String> history;
    private int count;
    private int size;

    {
        history = new ArrayList<String>();
        history.add("created");
    }

    public Outer(String name) {
        this.name = name;
    }

    public Outer() {
        this("anonymous");
    }

    public int size() {
        return size;
    }

    public Outer size(int size) {
        this.size = size;
        return this;
    }

    public class Counter {
        private int count;

        public int increment() {
            count++;
            Outer.this.count += 2;
            history.add("increment " + count);
            return count + Outer.this.count;
        }

        public String owner() {
            return name;
        }
    }

    public class Registry<T> {
        private final List<T> items = new ArrayList<T>();

        public Registry<T> add(T item) {
            items.add(item);
            return this;
        }

        public Iterator<String> labels() {
            return new Iterator<String>() {
                private int index;

                @Override
                public boolean hasNext() {
                    return index < items.size();
                }

                @Override
                public String next() {
                    final T item = items.get(index);
                    final int position = index++;
                    // render(item): only render(Object) applies; render(position): boxing picks render(Object).
                    Supplier<String> label = () -> render(item) + render(position) + "@" + name;
                    return label.get();
                }

                @Override
                public void remove() {
                    throw new UnsupportedOperationException("read-only");
                }
            };
        }
    }

    static String render(Object value) {
        return "object:" + value;
    }

    static String render(String value) {
        return "string:" + value;
    }

    public static final class Pair<A, B> {
        private final A first;
        private final B second;

        public Pair(A first, B second) {
            this.first = first;
            this.second = second;
        }

        public static <X> Pair<X, X> twin(X value) {
            return new Pair<X, X>(value, value);
        }

        public <C> Pair<A, C> withSecond(C value) {
            return new Pair<A, C>(first, value);
        }

        public A first() {
            return first;
        }

        public B second() {
            return second;
        }
    }

    public Runnable announcer(final String prefix) {
        final int snapshot = count;
        String suffix = "!";
        class Announcer implements Runnable {
            private int calls;

            @Override
            public void run() {
                calls++;
                history.add(prefix + name + snapshot + suffix + calls);
            }
        }
        return new Announcer();
    }

    public Comparator<String> byLength(final boolean descending) {
        return new Comparator<String>() {
            @Override
            public int compare(String left, String right) {
                int difference = left.length() - right.length();
                return descending ? -difference : difference;
            }
        };
    }

    /** The parameter shadows the field {@code count}; the local shadows the field {@code name} with another type. */
    public int shadow(int count) {
        int name = count * 2;
        return name + this.count + this.name.length();
    }

    public static int firstNegativeRow(int[][] grid) {
        int found = -1;
        search:
        for (int row = 0; row < grid.length; row++) {
            for (int column = 0; column < grid[row].length; column++) {
                if (grid[row][column] < 0) {
                    found = row;
                    break search;
                }
            }
        }
        return found;
    }

    private static String ownerOf(Outer.Counter counter) {
        return counter.owner();
    }

    public String demo() {
        Counter counter = new Counter();
        Outer.Counter other = this.new Counter();
        int total = counter.increment() + other.increment();
        Registry<Integer> registry = new Registry<Integer>();
        registry.add(1).add(2);
        StringBuilder labels = new StringBuilder();
        for (Iterator<String> it = registry.labels(); it.hasNext();) {
            labels.append(it.next());
        }
        announcer(">").run();
        List<String> words = new ArrayList<String>(history);
        Collections.sort(words, byLength(true));
        Pair<String, Integer> pair = Pair.twin("x").withSecond(DEFAULTS.get("retries"));
        int[][] grid = { { 1, 2 }, { 3, -4 } };
        return total + labels.toString() + words + pair.first() + pair.second()
                + shadow(size(4).size()) + ownerOf(counter) + firstNegativeRow(grid);
    }
}
