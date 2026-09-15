package corpus.legacy.util;

import java.util.Arrays;
import java.util.Collection;
import java.util.Iterator;

/** A static utility class in the old style, used through static imports. */
public final class StringUtil {

    public static final String EMPTY = "";

    private StringUtil() {
    }

    public static boolean is_blank(String value) {
        return value == null || value.trim().length() == 0;
    }

    public static String default_if_blank(String value, String fallback) {
        return is_blank(value) ? fallback : value;
    }

    public static String pad_left(String value, int width, char pad) {
        StringBuffer buffer = new StringBuffer(width);
        for (int i = value.length(); i < width; i++) {
            buffer.append(pad);
        }
        return buffer.append(value).toString();
    }

    public static String join(Collection items, String separator) {
        StringBuffer buffer = new StringBuffer();
        for (Iterator it = items.iterator(); it.hasNext();) {
            buffer.append(it.next());
            if (it.hasNext()) {
                buffer.append(separator);
            }
        }
        return buffer.toString();
    }

    public static String join(Object[] items, String separator) {
        return join(Arrays.asList(items), separator);
    }
}
