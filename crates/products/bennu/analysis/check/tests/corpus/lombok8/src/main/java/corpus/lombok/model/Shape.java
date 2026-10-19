package corpus.lombok.model;

import lombok.Getter;
import lombok.experimental.SuperBuilder;

/** Base of a {@code @SuperBuilder} hierarchy: the builders inherit each other's setters. */
@SuperBuilder
@Getter
public class Shape {
    private final String label;
}
