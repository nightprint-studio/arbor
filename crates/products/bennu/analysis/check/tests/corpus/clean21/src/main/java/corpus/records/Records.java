package corpus.records;

import java.util.Comparator;
import java.util.EnumMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.stream.Collectors;

/**
 * Generic records, records implementing interfaces, a record nested in a record, compact constructors,
 * static factories, and local records, enums and interfaces.
 */
public final class Records {

    private Records() {
    }

    public interface Identified<ID> {
        ID id();
    }

    public interface Priced {
        Money price();

        default boolean costlierThan(Priced other) {
            return price().compareTo(other.price()) > 0;
        }
    }

    public record Pair<A, B>(A first, B second) {
        public static <T> Pair<T, T> twin(T value) {
            return new Pair<>(value, value);
        }

        public <C> Pair<A, C> withSecond(C value) {
            return new Pair<>(first, value);
        }

        public Pair<B, A> swap() {
            return new Pair<>(second, first);
        }
    }

    public record Address(String street, String city, String zip) {
        public Address {
            zip = zip == null ? "" : zip.strip();
        }

        public boolean inCity(String other) {
            return city.equalsIgnoreCase(other);
        }
    }

    public record Customer(Long id, String name, Address address) implements Identified<Long> {
        public Customer {
            Objects.requireNonNull(id, "id");
            if (name == null || name.isBlank()) {
                throw new IllegalArgumentException("blank customer name");
            }
        }

        public static Customer of(long id, String name, String city) {
            return new Customer(id, name, new Address("", city, null));
        }

        public Customer rename(String newName) {
            return new Customer(id, newName, address);
        }
    }

    public record Product(String sku, Money price) implements Priced, Comparable<Product> {
        @Override
        public int compareTo(Product other) {
            return price.compareTo(other.price);
        }
    }

    public record Line(String sku, int quantity, Money unitPrice) {
        public Money total() {
            return unitPrice.times(quantity);
        }
    }

    public record Invoice(String number, List<Line> lines) {
        public Invoice {
            lines = List.copyOf(lines);
        }

        /** A record nested in a record, used only inside the enclosing record's body. */
        public record Totals(int items, Money amount) {
        }

        public Totals totals() {
            var items = 0;
            var amount = Money.of("0", "EUR");
            for (var line : lines) {
                items += line.quantity();
                amount = amount.plus(line.total());
            }
            return new Totals(items, amount);
        }
    }

    public static Map<String, Integer> topCities(List<Customer> customers, int limit) {
        record CityCount(String city, long count) {
        }
        return customers.stream()
                .collect(Collectors.groupingBy(customer -> customer.address().city(), Collectors.counting()))
                .entrySet()
                .stream()
                .map(entry -> new CityCount(entry.getKey(), entry.getValue()))
                .sorted(Comparator.comparingLong(CityCount::count).reversed().thenComparing(CityCount::city))
                .limit(limit)
                .collect(Collectors.toMap(
                        CityCount::city,
                        cityCount -> (int) cityCount.count(),
                        (left, right) -> left,
                        LinkedHashMap::new));
    }

    public static String bands(List<Integer> scores) {
        enum Band {
            LOW, MID, HIGH
        }
        interface Classifier {
            Band classify(int score);
        }
        Classifier classifier = score -> score < 50 ? Band.LOW : score < 80 ? Band.MID : Band.HIGH;
        var counts = new EnumMap<Band, Integer>(Band.class);
        for (int value : scores) {
            counts.merge(classifier.classify(value), 1, Integer::sum);
        }
        return counts.toString();
    }

    public static String demo() {
        var pair = Pair.twin("x").withSecond(42).swap();
        var customer = Customer.of(7, "Ada", "Turin").rename("Ada L.");
        var invoice = new Invoice("F-1", List.of(new Line("A", 2, Money.of("1.50", "EUR"))));
        var cheap = new Product("A", Money.of("1", "EUR"));
        var dear = new Product("B", Money.of("9", "EUR"));
        return pair.first() + pair.second() + customer.address().inCity("turin") + customer.id()
                + invoice.totals().amount() + dear.costlierThan(cheap) + cheap.compareTo(dear)
                + topCities(List.of(customer), 3) + bands(List.of(10, 60, 90));
    }
}
