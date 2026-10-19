package corpus.lombok.model;

import java.io.ByteArrayInputStream;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.List;
import lombok.Cleanup;
import lombok.SneakyThrows;
import lombok.val;

/** {@code val} takes the type of its initializer; {@code @SneakyThrows} hides a checked exception. */
public class Inferred {
    public int count(List<String> input) {
        val copy = new ArrayList<String>(input);
        val size = copy.size();
        return size + copy.get(0).length();
    }

    @SneakyThrows
    public int first(byte[] data) {
        @Cleanup InputStream in = new ByteArrayInputStream(data);
        return in.read();
    }
}
