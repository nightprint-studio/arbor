package corpus.lombok.model;

import java.util.List;
import lombok.Builder;
import lombok.Singular;
import lombok.Value;
import lombok.With;

/** An immutable value with a builder, a copy-with and a singular collection. */
@Value
@Builder(toBuilder = true)
public class Order {
    String id;
    @With
    long total;
    @Singular
    List<String> items;
}
