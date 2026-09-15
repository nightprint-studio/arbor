package corpus.resolve;

import java.lang.annotation.Retention;
import java.util.Comparator;
import java.util.List;
import java.util.Map.Entry;
import java.util.concurrent.*;
import static java.lang.Math.*;
import static java.lang.Math.max;
import static java.util.AbstractMap.SimpleEntry;
import static java.util.Collections.emptyList;
import static java.util.Map.Entry.comparingByKey;

/** Legal twins of {@link ResolveImportBad}: single, on-demand, nested and static imports, all used. */
public class ResolveImportOk {
    List<String> names = emptyList();
    Entry<String, Integer> entry = null;
    ExecutorService executor = null;
    int biggest = max(1, 2);
    double circle = PI * abs(-2.0);
    Comparator<Entry<String, Integer>> byKey = comparingByKey();
    SimpleEntry<String, String> pair = null;
    Class<Retention> retention = Retention.class;
}
