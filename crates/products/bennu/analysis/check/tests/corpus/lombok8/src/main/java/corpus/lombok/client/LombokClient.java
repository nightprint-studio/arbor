package corpus.lombok.client;

import corpus.lombok.model.Account;
import corpus.lombok.model.Circle;
import corpus.lombok.model.Customer;
import corpus.lombok.model.Inferred;
import corpus.lombok.model.Logged;
import corpus.lombok.model.Order;
import corpus.lombok.model.Settings;
import corpus.lombok.model.Wrapper;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/** Reads every member the model classes only declare through annotations. */
public class LombokClient {
    public String customer() {
        Customer customer = new Customer("Ada", 36, true);
        customer.setAge(customer.getAge() + 1);
        Customer blank = new Customer();
        blank.setName(customer.getName());
        blank.setActive(!customer.isActive());
        return blank + "/" + customer.equals(blank) + "/" + customer.hashCode();
    }

    public List<String> order() {
        Order order = Order.builder().id("o-1").total(10L).item("a").item("b").build();
        Order bigger = order.withTotal(order.getTotal() + 5).toBuilder().item("c").build();
        return bigger.getItems();
    }

    public String settings() {
        Settings settings = Settings.builder().retries(5).build();
        return settings.getCurrency() + settings.getRetries();
    }

    public int account() {
        Account account = new Account("id", "owner", "secret");
        account.balance(account.balance() + 1);
        return account.id().length() + account.owner().length();
    }

    public int inferred() {
        Logged logged = new Logged();
        logged.hello("world");
        Inferred inferred = new Inferred();
        return inferred.count(new ArrayList<String>(Arrays.asList("x"))) + inferred.first(new byte[] {1});
    }

    public int delegated() {
        Wrapper wrapper = new Wrapper(new ArrayList<String>());
        wrapper.add("x");
        return wrapper.size();
    }

    public double shape() {
        Circle circle = Circle.builder().label("c").radius(2.0).build();
        return circle.getRadius() + circle.getLabel().length();
    }
}
