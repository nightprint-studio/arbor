package corpus.lombok.model;

import lombok.Builder;
import lombok.Getter;

/** Builder defaults: the initializer is the default, the builder method overrides it. */
@Getter
@Builder
public class Settings {
    @Builder.Default
    private final String currency = "EUR";
    @Builder.Default
    private final int retries = 3;
}
