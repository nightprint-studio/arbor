package corpus.structure;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.function.Predicate;

/**
 * Level-21 structure mixed the way real code mixes it: an enum with abstract methods feeding a record,
 * a generic inner class creating an anonymous diamond iterator with a lambda inside, {@code Outer.this},
 * a local class capturing effectively-final {@code var}s, and an inner-class diamond creation.
 */
public class Inventory {

    public enum Category {
        FOOD {
            @Override
            public double taxRate() {
                return 0.04;
            }
        },
        TOOLS {
            @Override
            public double taxRate() {
                return 0.22;
            }
        };

        public abstract double taxRate();
    }

    public record Item(String name, Category category, int quantity, double price) {
        public double gross() {
            return quantity * price * (1 + category.taxRate());
        }
    }

    private final List<Item> items = new ArrayList<>();
    private final List<String> flagged = new ArrayList<>();
    private final String owner;

    public Inventory(String owner) {
        this.owner = owner;
    }

    public Inventory add(Item item) {
        items.add(item);
        return this;
    }

    public class View<T extends Item> implements Iterable<T> {
        private final Class<T> type;
        private final Predicate<? super T> filter;

        public View(Class<T> type, Predicate<? super T> filter) {
            this.type = type;
            this.filter = filter;
        }

        @Override
        public Iterator<T> iterator() {
            var matching = new ArrayList<T>();
            for (var candidate : items) {
                if (type.isInstance(candidate) && filter.test(type.cast(candidate))) {
                    matching.add(type.cast(candidate));
                }
            }
            return new Iterator<>() {
                private int index;

                @Override
                public boolean hasNext() {
                    return index < matching.size();
                }

                @Override
                public T next() {
                    var item = matching.get(index++);
                    Predicate<T> expensive = entry -> entry.gross() > 100;
                    if (expensive.test(item)) {
                        flagged.add(item.name());
                    }
                    return item;
                }
            };
        }

        public String owner() {
            return Inventory.this.owner;
        }
    }

    public String summary() {
        var food = 0;
        var tools = 0;
        for (var item : items) {
            switch (item.category()) {
                case FOOD -> food += item.quantity();
                case TOOLS -> tools += item.quantity();
            }
        }
        return owner + ": food=" + food + ", tools=" + tools;
    }

    public double total(Category category) {
        var threshold = switch (category) {
            case FOOD -> 0.0;
            case TOOLS -> 10.0;
        };
        class Accumulator {
            private double sum;

            void accept(Item item) {
                if (item.category() == category && item.price() >= threshold) {
                    sum += item.gross();
                }
            }
        }
        var accumulator = new Accumulator();
        items.forEach(accumulator::accept);
        return accumulator.sum;
    }

    public static String demo() {
        var inventory = new Inventory("shop")
                .add(new Item("bread", Category.FOOD, 10, 2.5))
                .add(new Item("saw", Category.TOOLS, 2, 60));
        var view = inventory.new View<>(Item.class, item -> item.quantity() > 1);
        var names = new StringBuilder(view.owner());
        for (var item : view) {
            names.append(' ').append(item.name());
        }
        return names + " " + inventory.summary() + " " + inventory.total(Category.TOOLS) + " " + inventory.flagged;
    }
}
