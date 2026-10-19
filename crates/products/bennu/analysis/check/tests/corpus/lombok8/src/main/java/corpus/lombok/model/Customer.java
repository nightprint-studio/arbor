package corpus.lombok.model;

import lombok.AllArgsConstructor;
import lombok.Data;
import lombok.NoArgsConstructor;

/** The classic bean: accessors, equals/hashCode/toString and both constructors, all generated. */
@Data
@NoArgsConstructor
@AllArgsConstructor
public class Customer {
    private String name;
    private int age;
    private boolean active;
}
