package corpus.lombok.model;

import java.util.List;
import lombok.AccessLevel;
import lombok.AllArgsConstructor;
import lombok.experimental.Delegate;
import lombok.experimental.FieldDefaults;

/** {@code @Delegate} copies every public method of the field's type onto the wrapper. */
@FieldDefaults(level = AccessLevel.PRIVATE, makeFinal = true)
@AllArgsConstructor
public class Wrapper {
    @Delegate
    List<String> inner;
}
