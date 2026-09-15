package corpus.generics;

import java.util.Arrays;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;
import java.util.function.Function;
import java.util.stream.Collectors;
import java.util.stream.Stream;

/** Type inference through Stream / Collectors chains, the way reporting code is written. */
public class StreamInference {

    public static final class Order {
        private final String customer;
        private final String city;
        private final int amount;

        public Order(String customer, String city, int amount) {
            this.customer = customer;
            this.city = city;
            this.amount = amount;
        }

        public String customer() {
            return customer;
        }

        public String city() {
            return city;
        }

        public int amount() {
            return amount;
        }
    }

    public static List<Order> sample() {
        return Arrays.asList(
                new Order("ann", "Rome", 120),
                new Order("bob", "Milan", 80),
                new Order("ann", "Milan", 45),
                new Order("cid", "Rome", 300));
    }

    public static Map<String, List<Order>> byCity(List<Order> orders) {
        return orders.stream().collect(Collectors.groupingBy(Order::city));
    }

    public static Map<String, Long> countByCity(List<Order> orders) {
        return orders.stream().collect(Collectors.groupingBy(Order::city, Collectors.counting()));
    }

    public static TreeMap<String, Integer> totalByCustomer(List<Order> orders) {
        return orders.stream().collect(Collectors.groupingBy(
                Order::customer,
                TreeMap::new,
                Collectors.reducing(0, Order::amount, Integer::sum)));
    }

    public static Map<String, Integer> amountByCustomer(List<Order> orders) {
        return orders.stream().collect(Collectors.toMap(Order::customer, Order::amount, Integer::sum));
    }

    public static Map<String, Map<String, Integer>> amountByCityAndCustomer(List<Order> orders) {
        return orders.stream().collect(Collectors.groupingBy(
                Order::city,
                Collectors.groupingBy(Order::customer, Collectors.summingInt(Order::amount))));
    }

    public static Map<Boolean, List<String>> partition(List<Order> orders, final int threshold) {
        return orders.stream().collect(Collectors.partitioningBy(
                order -> order.amount() >= threshold,
                Collectors.mapping(Order::customer, Collectors.toList())));
    }

    public static Optional<Order> biggest(List<Order> orders) {
        return orders.stream().max(Comparator.comparingInt(Order::amount));
    }

    public static List<Order> sortedByCityThenCustomer(List<Order> orders) {
        return orders.stream()
                .sorted(Comparator.comparing(Order::city).thenComparing(Order::customer))
                .collect(Collectors.toList());
    }

    public static String cities(List<Order> orders) {
        return orders.stream()
                .map(Order::city)
                .distinct()
                .sorted()
                .collect(Collectors.joining(", ", "[", "]"));
    }

    public static Optional<String> longestCustomer(List<Order> orders) {
        return orders.stream()
                .map(Order::customer)
                .reduce((left, right) -> left.length() >= right.length() ? left : right);
    }

    public static List<String> frozenCustomers(List<Order> orders) {
        return orders.stream()
                .map(Order::customer)
                .collect(Collectors.collectingAndThen(Collectors.toList(), Collections::unmodifiableList));
    }

    public static List<String> flatten(List<List<String>> groups) {
        return groups.stream().flatMap(List::stream).collect(Collectors.toList());
    }

    public static int sumOfSquares(List<Integer> values) {
        return values.stream().mapToInt(Integer::intValue).map(value -> value * value).sum();
    }

    /** A wildcard-typed key extractor flowing into {@code groupingBy}. */
    public static <T, K> Map<K, List<T>> index(Stream<T> items, Function<? super T, ? extends K> key) {
        return items.collect(Collectors.groupingBy(key));
    }

    public static <T, V> Map<T, V> measure(List<T> items, Function<? super T, V> measure) {
        return items.stream().collect(Collectors.toMap(Function.identity(), measure));
    }

    /** Static generic factories called with explicit type arguments. */
    public static String explicitTypeArguments() {
        List<String> empty = Collections.<String>emptyList();
        Map<String, Integer> lengths = StreamInference.<String, Integer>measure(Arrays.asList("a", "bb"), String::length);
        Map<Integer, List<String>> byLength = StreamInference.<String, Integer>index(Stream.of("x", "yy", "zz"), String::length);
        Comparator<String> natural = Comparator.<String>naturalOrder();
        return empty.size() + " " + lengths + " " + byLength + " " + natural.compare("a", "b");
    }

    public static String report() {
        List<Order> orders = sample();
        StringBuilder out = new StringBuilder();
        out.append(byCity(orders).keySet())
                .append(countByCity(orders))
                .append(totalByCustomer(orders).firstKey())
                .append(amountByCustomer(orders))
                .append(amountByCityAndCustomer(orders))
                .append(partition(orders, 100).get(Boolean.TRUE))
                .append(sortedByCityThenCustomer(orders).size())
                .append(cities(orders))
                .append(longestCustomer(orders).orElse("-"))
                .append(frozenCustomers(orders))
                .append(flatten(Arrays.asList(Arrays.asList("a"), Arrays.asList("b", "c"))))
                .append(sumOfSquares(Arrays.asList(1, 2, 3)));
        Optional<Order> top = biggest(orders);
        if (top.isPresent()) {
            out.append(top.get().customer());
        }
        return out.append(explicitTypeArguments()).toString();
    }
}
