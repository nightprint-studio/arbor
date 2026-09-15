package corpus.legacy;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.Enumeration;
import java.util.HashMap;
import java.util.Hashtable;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Vector;

/**
 * Pre-generics collection code as found in long-lived Struts-era projects: Vector, Hashtable,
 * Enumeration, raw collections with casts, anonymous raw Comparators, StringBuffer, snake_case names.
 * Raw types and unchecked calls are warnings (javac notes), never errors.
 */
public class LegacyCollections {

    private Vector rows = new Vector();
    private Hashtable index_by_code = new Hashtable();
    private int modification_count;

    public void add_row(String code, String description) {
        String[] row = new String[] { code, description };
        rows.addElement(row);
        index_by_code.put(code, row);
        modification_count++;
    }

    public String description_of(String code) {
        String[] row = (String[]) index_by_code.get(code);
        return row == null ? null : row[1];
    }

    public List codes_sorted() {
        List codes = new ArrayList();
        Enumeration keys = index_by_code.keys();
        while (keys.hasMoreElements()) {
            codes.add(keys.nextElement());
        }
        Collections.sort(codes, new Comparator() {
            public int compare(Object a, Object b) {
                return ((String) a).compareToIgnoreCase((String) b);
            }
        });
        return codes;
    }

    public String join_descriptions(String separator) {
        StringBuffer buffer = new StringBuffer();
        for (Iterator it = rows.iterator(); it.hasNext();) {
            String[] row = (String[]) it.next();
            if (buffer.length() > 0) {
                buffer.append(separator);
            }
            buffer.append(row[1]);
        }
        return buffer.toString();
    }

    public Map count_by_prefix() {
        Map counts = new HashMap();
        for (int i = 0; i < rows.size(); i++) {
            String code = ((String[]) rows.elementAt(i))[0];
            String prefix = code.length() > 2 ? code.substring(0, 2) : code;
            Integer current = (Integer) counts.get(prefix);
            counts.put(prefix, Integer.valueOf(current == null ? 1 : current.intValue() + 1));
        }
        return counts;
    }

    public static Vector to_vector(Object[] items) {
        Vector vector = new Vector(items.length);
        for (int i = 0; i < items.length; i++)
            vector.addElement(items[i]);
        return vector;
    }

    /** Raw and parameterized types meeting: an unchecked conversion. */
    public List<String> typed_codes() {
        List<String> typed = new ArrayList<String>(index_by_code.keySet());
        Collections.sort(typed);
        return typed;
    }

    public Enumeration descriptions() {
        final Iterator source = rows.iterator();
        return new Enumeration() {
            public boolean hasMoreElements() {
                return source.hasNext();
            }

            public Object nextElement() {
                return ((String[]) source.next())[1];
            }
        };
    }

    public int modification_count() {
        return modification_count;
    }

    public static String dump(LegacyCollections table) {
        StringBuffer out = new StringBuffer();
        for (Enumeration e = table.descriptions(); e.hasMoreElements();) {
            out.append(e.nextElement()).append(';');
        }
        Iterator entries = table.count_by_prefix().entrySet().iterator();
        while (entries.hasNext()) {
            Map.Entry entry = (Map.Entry) entries.next();
            out.append(entry.getKey()).append('=').append(entry.getValue());
        }
        Vector copy = to_vector(table.typed_codes().toArray());
        return out.toString() + table.codes_sorted() + table.modification_count() + copy.size()
                + table.join_descriptions("|") + table.description_of("A1");
    }
}
