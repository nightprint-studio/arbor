package corpus.structure;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import corpus.structure.base.Entity;

/** Subclass of a generic base class from another package, reaching its protected members legally. */
public class Customer extends Entity<Long> {

    private String email;

    public Customer(long id, String email) {
        super(id);
        this.email = email;
    }

    @Override
    public String kind() {
        return "customer";
    }

    public String email() {
        return email;
    }

    public void email(String email) {
        this.email = email;
        touch();
    }

    public int revision() {
        return version;
    }

    /** Protected access through a qualifier of the subclass's own type is allowed. */
    public boolean newerThan(Customer other) {
        return version > other.version;
    }

    public static List<Customer> sortById(List<Customer> customers) {
        List<Customer> copy = new ArrayList<Customer>(customers);
        Collections.sort(copy);
        return copy;
    }

    /** An anonymous subclass may call a protected constructor from another package. */
    public static Entity<String> tag(String value) {
        return new Entity<String>(value) {
            @Override
            public String kind() {
                return "tag";
            }
        };
    }

    public static String demo() {
        Customer first = new Customer(2L, "a@example.org");
        Customer second = new Customer(1, "b@example.org");
        second.email("c@example.org");
        List<Customer> sorted = sortById(Collections.<Customer>emptyList());
        return first.getId() + second.email() + second.revision() + second.newerThan(first)
                + sorted.size() + tag("vip") + first.compareTo(second);
    }
}
