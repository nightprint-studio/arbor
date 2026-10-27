package corpus.legacy;

import java.util.Arrays;
import java.util.Enumeration;
import java.util.HashMap;
import java.util.Hashtable;
import java.util.Iterator;
import java.util.Map;
import java.util.Properties;
import java.util.Vector;

/** Pre-generics and pre-Java-7 habits that a legacy code base is full of, all of which compile. */
@SuppressWarnings({"rawtypes", "unchecked", "deprecation"})
public class OldSchool {
    public static final int MAX = 010 + 0x1F + 0b11;
    static final String NAMES[] = {"a", "b"};
    private static int counter;
    private transient volatile boolean dirty;
    private final Object lock = new Object();

    static {
        counter = NAMES.length;
    }

    {
        dirty = false;
    }

    /** A C-style method return: the brackets follow the parameter list. */
    String[] lines()[] {
        return new String[][] {NAMES, NAMES};
    }

    int sums(int first, int rest[]) [] {
        return rest;
    }

    int mixed() {
        int plain, grid[], more[][];
        plain = 1;
        grid = new int[] {1, 2};
        more = new int[][] {grid};
        return plain + grid[0] + more[0][1];
    }

    Vector toVector(Hashtable table) {
        Vector out = new Vector();
        Enumeration keys = table.keys();
        while (keys.hasMoreElements()) {
            String key = (String) keys.nextElement();
            out.addElement(key + "=" + table.get(key));
        }
        return out;
    }

    String firstOf(Vector v) {
        if (v.isEmpty()) {
            return null;
        }
        return (String) v.elementAt(0);
    }

    int total(Map map) {
        int sum = 0;
        for (Iterator it = map.entrySet().iterator(); it.hasNext();) {
            Map.Entry entry = (Map.Entry) it.next();
            Integer value = (Integer) entry.getValue();
            sum += value.intValue();
        }
        return sum;
    }

    Integer boxes() {
        Integer a = new Integer(5);
        Long b = new Long("7");
        Boolean c = Boolean.valueOf("true");
        return c.booleanValue() ? new Integer(a.intValue() + b.intValue()) : a;
    }

    Runnable anonymous(final String label) {
        return new Runnable() {
            public void run() {
                System.out.println(label + counter);
            }
        };
    }

    java.util.Comparator byLength() {
        return new java.util.Comparator() {
            public int compare(Object a, Object b) {
                return ((String) a).length() - ((String) b).length();
            }
        };
    }

    int search(final int[][] grid, final int wanted) {
        int found = -1;
        outer:
        for (int i = 0; i < grid.length; i++) {
            for (int j = 0; j < grid[i].length; j++) {
                if (grid[i][j] == wanted) {
                    found = i * 10 + j;
                    break outer;
                }
                if (grid[i][j] < 0) {
                    continue outer;
                }
            }
        }
        return found;
    }

    String describe(int code, String kind) {
        StringBuffer sb = new StringBuffer();
        switch (code) {
            case 1:
                sb.append("one");
                break;
            case 2:
                sb.append('t').append(2);
                break;
            default:
                sb.append(code);
        }
        switch (kind) {
            case "x":
                sb.append("X");
                break;
            default:
                break;
        }
        char c = 'a';
        c += 1;
        byte b = 10;
        b *= 2;
        short s = (short) (b + c);
        long big = 1L << 40;
        sb.append(s).append(big);
        return sb.toString();
    }

    Object[] spread(String first, Object... rest) {
        Object all[] = new Object[rest.length + 1];
        all[0] = first;
        System.arraycopy(rest, 0, all, 1, rest.length);
        return all;
    }

    void callSpread() {
        Object[] a = spread("a");
        Object[] b = spread("a", 1, "two");
        Object[] c = spread("a", new Object[] {1, 2});
        String d = String.format("%s-%s", a.length, b.length > 0 ? "yes" : "no");
        Arrays.sort(c);
        System.out.println(d);
    }

    String lookup(Properties props, String key) {
        synchronized (lock) {
            String value = props.getProperty(key);
            return value != null ? value : props.getProperty(key + ".default", "none");
        }
    }

    class Inner {
        int bump() {
            return OldSchool.this.mixed() + counter++;
        }
    }

    int viaInner() {
        return new Inner().bump();
    }

    HashMap copy(HashMap source) {
        HashMap target = new HashMap();
        target.putAll(source);
        return target;
    }

    Class loadIt(String name) {
        try {
            return Class.forName(name);
        } catch (ClassNotFoundException e) {
            throw new IllegalStateException(e.getMessage());
        } finally {
            counter++;
        }
    }

    boolean check(Object o) {
        assert o != null : "never null";
        if (o instanceof String) {
            String str = (String) o;
            return str.length() > 0;
        }
        return o instanceof Number && ((Number) o).intValue() > 0;
    }
}
