package corpus.lombok.model;

import lombok.Getter;
import lombok.experimental.SuperBuilder;

/** Its builder carries {@code label(...)} from {@link Shape}'s. */
@SuperBuilder
@Getter
public class Circle extends Shape {
    private final double radius;
}
