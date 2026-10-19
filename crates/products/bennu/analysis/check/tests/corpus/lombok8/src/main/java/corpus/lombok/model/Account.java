package corpus.lombok.model;

import lombok.EqualsAndHashCode;
import lombok.Getter;
import lombok.NonNull;
import lombok.RequiredArgsConstructor;
import lombok.Setter;
import lombok.ToString;
import lombok.experimental.Accessors;

/** Fluent accessors (`id()`, `balance(int)`), a constructor over the final fields, selective equals. */
@Getter
@ToString(exclude = "secret")
@EqualsAndHashCode(of = "id")
@RequiredArgsConstructor
@Accessors(fluent = true)
public class Account {
    private final String id;
    @NonNull
    private final String owner;
    private final String secret;
    @Setter
    private int balance;
}
