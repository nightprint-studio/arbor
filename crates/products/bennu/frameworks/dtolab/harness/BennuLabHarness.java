import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.lang.annotation.Annotation;
import java.lang.reflect.Array;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.ParameterizedType;
import java.lang.reflect.Type;
import java.math.BigDecimal;
import java.math.BigInteger;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.Calendar;
import java.util.Collection;
import java.util.Collections;
import java.util.Date;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.UUID;

/**
 * Bennu DTO Lab harness — answers questions about a project's classes by running them: on the
 * project's own JDK, with the project's own Jackson and Bean Validation.
 *
 * It depends on nothing but the JDK and loads the project into a class loader of its own, so the
 * harness is never on the project's classpath, and a rebuilt project is picked up by replacing that
 * loader rather than restarting the JVM.
 *
 * Protocol: one request per line, id TAB operation TAB base64(argument)...; one reply per line,
 * id TAB json. Every reply has "ok"; a failure carries "error". Written for Java 8, the oldest JDK
 * a project this runs for may use.
 */
public final class BennuLabHarness {

    private static PrintStream protocol;
    private static URLClassLoader loader;
    private static String epoch = "";
    private static String interpolatorNote;
    private static final Map<String, Object> cache = new HashMap<String, Object>();

    private BennuLabHarness() {
    }

    public static void main(String[] args) throws Exception {
        // stdout is the protocol. Whatever the project prints — a banner in a static initialiser, a
        // console appender — goes to stderr instead of into a reply.
        protocol = new PrintStream(new FileOutputStream(FileDescriptor.out), true, "UTF-8");
        System.setOut(System.err);
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String line;
        while ((line = in.readLine()) != null) {
            if (line.isEmpty()) {
                continue;
            }
            String[] parts = line.split("\t", -1);
            String reply;
            try {
                String[] arguments = new String[Math.max(0, parts.length - 2)];
                for (int i = 2; i < parts.length; i++) {
                    arguments[i - 2] = new String(Base64.getDecoder().decode(parts[i]), StandardCharsets.UTF_8);
                }
                reply = dispatch(parts.length > 1 ? parts[1] : "", arguments);
            } catch (Throwable failure) {
                reply = "{\"ok\":false,\"error\":" + str(explain(failure)) + "}";
            }
            protocol.print(parts[0] + "\t" + reply + "\n");
            protocol.flush();
        }
    }

    private static String dispatch(String operation, String[] a) throws Exception {
        if ("ping".equals(operation)) {
            return "{\"ok\":true}";
        }
        if ("classpath".equals(operation)) {
            return classpath(a[0], a[1]);
        }
        if (loader == null) {
            throw new IllegalStateException("the project's classpath has not been set");
        }
        Thread.currentThread().setContextClassLoader(loader);
        if ("read".equals(operation)) {
            return read(a[0], a[1], flag(a, 2));
        }
        if ("default".equals(operation)) {
            return defaults(a[0], flag(a, 1));
        }
        if ("validate".equals(operation)) {
            return validate(a[0], a[1], a[2], a[3], flag(a, 4), a.length > 5 ? a[5] : "");
        }
        if ("describe".equals(operation)) {
            return describeClass(a[0], a.length > 1 ? a[1] : "");
        }
        if ("constants".equals(operation)) {
            return constants(a[0]);
        }
        throw new IllegalArgumentException("unknown operation: " + operation);
    }

    private static boolean flag(String[] a, int index) {
        return a.length > index && "true".equals(a[index]);
    }

    // ── the project's classes ──────────────────────────────────────────────────────────────────

    private static String classpath(String newEpoch, String paths) throws Exception {
        if (loader != null && newEpoch.equals(epoch)) {
            return "{\"ok\":true}";
        }
        List<URL> urls = new ArrayList<URL>();
        for (String path : paths.split(File.pathSeparator)) {
            if (!path.isEmpty()) {
                urls.add(new File(path).toURI().toURL());
            }
        }
        if (loader != null) {
            try {
                loader.close();
            } catch (IOException ignored) {
                // An old loader that will not close is garbage all the same.
            }
        }
        // The parent is the loader ABOVE the application one, so nothing of the harness is visible to
        // the project — and nothing of an old project survives a rebuild.
        loader = new URLClassLoader(urls.toArray(new URL[0]), ClassLoader.getSystemClassLoader().getParent());
        epoch = newEpoch;
        cache.clear();
        return "{\"ok\":true}";
    }

    private static Class<?> type(String binary) throws ClassNotFoundException {
        return Class.forName(binary, true, loader);
    }

    // ── Jackson ────────────────────────────────────────────────────────────────────────────────

    /** The project's ObjectMapper, or null when the project has no Jackson. */
    private static Object mapper(boolean springBoot) throws Exception {
        String key = "mapper|" + springBoot;
        if (cache.containsKey(key)) {
            return cache.get(key);
        }
        Object mapper;
        try {
            Class<?> type = Class.forName("com.fasterxml.jackson.databind.ObjectMapper", true, loader);
            mapper = type.getConstructor().newInstance();
            try {
                type.getMethod("findAndRegisterModules").invoke(mapper);
            } catch (Throwable moduleFailed) {
                // One module that fails to load is not a reason to have none.
            }
            if (springBoot) {
                // What Spring Boot's auto-configuration changes from Jackson's own defaults.
                Class<?> deserialization = Class.forName("com.fasterxml.jackson.databind.DeserializationFeature", true, loader);
                type.getMethod("configure", deserialization, boolean.class)
                    .invoke(mapper, constant(deserialization, "FAIL_ON_UNKNOWN_PROPERTIES"), false);
                Class<?> serialization = Class.forName("com.fasterxml.jackson.databind.SerializationFeature", true, loader);
                type.getMethod("configure", serialization, boolean.class)
                    .invoke(mapper, constant(serialization, "WRITE_DATES_AS_TIMESTAMPS"), false);
            }
        } catch (ClassNotFoundException noJackson) {
            mapper = null;
        }
        cache.put(key, mapper);
        return mapper;
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static Object constant(Class<?> type, String name) {
        return Enum.valueOf((Class) type, name);
    }

    private static Object readValue(Object mapper, String json, Class<?> type) throws Exception {
        return mapper.getClass().getMethod("readValue", String.class, Class.class).invoke(mapper, json, type);
    }

    private static String write(Object mapper, Object value) throws Exception {
        Object writer = mapper.getClass().getMethod("writerWithDefaultPrettyPrinter").invoke(mapper);
        return (String) writer.getClass().getMethod("writeValueAsString", Object.class).invoke(writer, value);
    }

    private static String read(String binary, String json, boolean springBoot) throws Exception {
        Class<?> cls = type(binary);
        Object mapper = mapper(springBoot);
        StringBuilder reply = new StringBuilder("{\"ok\":true");
        Object instance;
        if (mapper == null) {
            instance = bind(cls, Json.parse(json));
            reply.append(",\"binder\":\"fields\",\"json\":null");
        } else {
            reply.append(",\"binder\":\"jackson\"");
            try {
                instance = readValue(mapper, json, cls);
            } catch (InvocationTargetException failed) {
                return reply.append(",\"object\":null,\"json\":null,\"binding_error\":")
                    .append(str(explain(failed))).append("}").toString();
            }
            try {
                reply.append(",\"json\":").append(str(write(mapper, instance)));
            } catch (InvocationTargetException failed) {
                reply.append(",\"json\":null,\"writing_error\":").append(str(explain(failed)));
            }
        }
        reply.append(",\"object\":").append(tree(null, instance, 0, new IdentityHashMap<Object, Boolean>()));
        return reply.append("}").toString();
    }

    private static String defaults(String binary, boolean springBoot) throws Exception {
        Class<?> cls = type(binary);
        Object mapper = mapper(springBoot);
        if (mapper == null) {
            return "{\"ok\":true,\"json\":null}";
        }
        Constructor<?> constructor = cls.getDeclaredConstructor();
        constructor.setAccessible(true);
        return "{\"ok\":true,\"json\":" + str(write(mapper, constructor.newInstance())) + "}";
    }

    // ── Bean Validation ────────────────────────────────────────────────────────────────────────

    private static String namespace(String preferred) {
        String[] candidates = preferred.isEmpty()
            ? new String[] {"jakarta.validation", "javax.validation"}
            : new String[] {preferred, "jakarta.validation", "javax.validation"};
        for (String candidate : candidates) {
            try {
                Class.forName(candidate + ".Validation", false, loader);
                return candidate;
            } catch (ClassNotFoundException next) {
                // try the other one
            }
        }
        throw new IllegalStateException("Bean Validation is not on the project's classpath");
    }

    private static Object validator(String ns, String locale, String bundles) throws Exception {
        String key = "validator|" + ns + "|" + locale + "|" + bundles;
        if (cache.containsKey(key)) {
            return cache.get(key);
        }
        Locale previous = Locale.getDefault();
        if (!locale.isEmpty()) {
            Locale.setDefault(Locale.forLanguageTag(locale));
        }
        try {
            Object bootstrap = Class.forName(ns + ".Validation", true, loader).getMethod("byDefaultProvider").invoke(null);
            Object configuration = Class.forName(ns + ".bootstrap.GenericBootstrap", true, loader)
                .getMethod("configure").invoke(bootstrap);
            Class<?> configurationType = Class.forName(ns + ".Configuration", true, loader);
            Object interpolator = interpolator(bundles);
            if (interpolator != null) {
                Class<?> interpolatorType = Class.forName(ns + ".MessageInterpolator", true, loader);
                configurationType.getMethod("messageInterpolator", interpolatorType).invoke(configuration, interpolator);
            }
            Object factory = configurationType.getMethod("buildValidatorFactory").invoke(configuration);
            Object validator = Class.forName(ns + ".ValidatorFactory", true, loader).getMethod("getValidator").invoke(factory);
            cache.put(key, validator);
            return validator;
        } finally {
            Locale.setDefault(previous);
        }
    }

    /** The interpolator for the bundles the project redirects messages to, or null for the provider's default. */
    private static Object interpolator(String bundles) {
        interpolatorNote = null;
        if (bundles.isEmpty()) {
            return null;
        }
        try {
            Class<?> locatorType = Class.forName("org.hibernate.validator.spi.resourceloading.ResourceBundleLocator", true, loader);
            String[] names = bundles.split(",");
            Object locator = names.length == 1
                ? Class.forName("org.hibernate.validator.resourceloading.PlatformResourceBundleLocator", true, loader)
                    .getConstructor(String.class).newInstance(names[0])
                : Class.forName("org.hibernate.validator.resourceloading.AggregateResourceBundleLocator", true, loader)
                    .getConstructor(List.class).newInstance(Arrays.asList(names));
            return Class.forName("org.hibernate.validator.messageinterpolation.ResourceBundleMessageInterpolator", true, loader)
                .getConstructor(locatorType).newInstance(locator);
        } catch (Throwable failed) {
            interpolatorNote = "The project's message bundles (" + bundles + ") could not be applied, so messages come from "
                + "the provider's defaults: " + explain(failed);
            return null;
        }
    }

    private static String validate(String binary, String json, String locale, String bundles, boolean springBoot,
                                   String preferred) throws Exception {
        Class<?> cls = type(binary);
        String ns = namespace(preferred);
        StringBuilder reply = new StringBuilder("{\"ok\":true");
        Object instance;
        Object mapper = mapper(springBoot);
        if (mapper != null) {
            reply.append(",\"binder\":\"jackson\"");
            try {
                instance = readValue(mapper, json, cls);
            } catch (InvocationTargetException failed) {
                return reply.append(",\"violations\":[],\"binding_error\":").append(str(explain(failed))).append("}").toString();
            }
        } else {
            reply.append(",\"binder\":\"fields\"");
            instance = bind(cls, Json.parse(json));
        }
        Object validator = validator(ns, locale, bundles);
        Locale previous = Locale.getDefault();
        if (!locale.isEmpty()) {
            Locale.setDefault(Locale.forLanguageTag(locale));
        }
        Set<?> found;
        try {
            found = (Set<?>) Class.forName(ns + ".Validator", true, loader)
                .getMethod("validate", Object.class, Class[].class).invoke(validator, instance, new Class<?>[0]);
        } finally {
            Locale.setDefault(previous);
        }
        Class<?> violationType = Class.forName(ns + ".ConstraintViolation", true, loader);
        Class<?> descriptorType = Class.forName(ns + ".metadata.ConstraintDescriptor", true, loader);
        List<String> rows = new ArrayList<String>();
        for (Object violation : found) {
            Object descriptor = violationType.getMethod("getConstraintDescriptor").invoke(violation);
            Annotation annotation = (Annotation) descriptorType.getMethod("getAnnotation").invoke(descriptor);
            Map<?, ?> attributes = (Map<?, ?>) descriptorType.getMethod("getAttributes").invoke(descriptor);
            Object invalid = violationType.getMethod("getInvalidValue").invoke(violation);
            rows.add("{\"path\":" + str(String.valueOf(violationType.getMethod("getPropertyPath").invoke(violation)))
                + ",\"template\":" + str((String) violationType.getMethod("getMessageTemplate").invoke(violation))
                + ",\"message\":" + str((String) violationType.getMethod("getMessage").invoke(violation))
                + ",\"constraint\":" + str(annotation.annotationType().getName())
                + ",\"attributes\":" + attributes(attributes)
                + ",\"invalid_value\":" + (invalid == null ? "null" : str(truncate(text(invalid))))
                + "}");
        }
        Collections.sort(rows);
        reply.append(",\"violations\":[").append(String.join(",", rows)).append("]");
        if (interpolatorNote != null) {
            reply.append(",\"note\":").append(str(interpolatorNote));
        }
        return reply.append("}").toString();
    }

    private static String describeClass(String binary, String preferred) throws Exception {
        Class<?> cls = type(binary);
        String ns = namespace(preferred);
        Object validator = validator(ns, "", "");
        Object bean = Class.forName(ns + ".Validator", true, loader)
            .getMethod("getConstraintsForClass", Class.class).invoke(validator, cls);
        Class<?> beanType = Class.forName(ns + ".metadata.BeanDescriptor", true, loader);
        Class<?> propertyType = Class.forName(ns + ".metadata.PropertyDescriptor", true, loader);
        Class<?> elementType = Class.forName(ns + ".metadata.ElementDescriptor", true, loader);
        Class<?> constraintType = Class.forName(ns + ".metadata.ConstraintDescriptor", true, loader);
        List<String> rows = new ArrayList<String>();
        for (Object property : (Set<?>) beanType.getMethod("getConstrainedProperties").invoke(bean)) {
            String name = (String) propertyType.getMethod("getPropertyName").invoke(property);
            Class<?> element = (Class<?>) elementType.getMethod("getElementClass").invoke(property);
            List<String> constraints = new ArrayList<String>();
            for (Object constraint : (Set<?>) elementType.getMethod("getConstraintDescriptors").invoke(property)) {
                Annotation annotation = (Annotation) constraintType.getMethod("getAnnotation").invoke(constraint);
                Map<?, ?> attributes = (Map<?, ?>) constraintType.getMethod("getAttributes").invoke(constraint);
                Object template = attributes.get("message");
                try {
                    template = constraintType.getMethod("getMessageTemplate").invoke(constraint);
                } catch (NoSuchMethodException beforeTwoPointZero) {
                    // Bean Validation 1.x has no getMessageTemplate; the attribute is the same text.
                }
                constraints.add("{\"annotation\":" + str(annotation.annotationType().getName())
                    + ",\"attributes\":" + attributes(attributes)
                    + ",\"template\":" + str(template == null ? "" : String.valueOf(template)) + "}");
            }
            rows.add("{\"name\":" + str(name) + ",\"type_name\":" + str(element.getSimpleName())
                + ",\"constraints\":[" + String.join(",", constraints) + "]}");
        }
        Collections.sort(rows);
        return "{\"ok\":true,\"properties\":[" + String.join(",", rows) + "]}";
    }

    /** Every public static final String of the named classes, keyed by its value. The first class wins a tie. */
    private static String constants(String classes) throws Exception {
        Map<String, String> byValue = new LinkedHashMap<String, String>();
        for (String name : classes.split(",")) {
            if (name.trim().isEmpty()) {
                continue;
            }
            Class<?> cls = type(name.trim());
            String owner = cls.getPackage() == null
                ? cls.getName()
                : cls.getName().substring(cls.getPackage().getName().length() + 1);
            owner = owner.replace('$', '.');
            for (Field field : cls.getFields()) {
                int modifiers = field.getModifiers();
                if (field.getType() != String.class || !Modifier.isStatic(modifiers) || !Modifier.isFinal(modifiers)) {
                    continue;
                }
                Object value = field.get(null);
                if (value != null && !byValue.containsKey(value)) {
                    byValue.put((String) value, owner + "." + field.getName());
                }
            }
        }
        StringBuilder out = new StringBuilder("{\"ok\":true,\"constants\":{");
        boolean first = true;
        for (Map.Entry<String, String> entry : byValue.entrySet()) {
            if (!first) {
                out.append(',');
            }
            first = false;
            out.append(str(entry.getKey())).append(':').append(str(entry.getValue()));
        }
        return out.append("}}").toString();
    }

    private static String attributes(Map<?, ?> attributes) {
        TreeMap<String, String> sorted = new TreeMap<String, String>();
        for (Map.Entry<?, ?> entry : attributes.entrySet()) {
            String key = String.valueOf(entry.getKey());
            if (key.equals("message") || key.equals("groups") || key.equals("payload")) {
                continue;
            }
            sorted.put(key, text(entry.getValue()));
        }
        StringBuilder out = new StringBuilder("{");
        boolean first = true;
        for (Map.Entry<String, String> entry : sorted.entrySet()) {
            if (!first) {
                out.append(',');
            }
            first = false;
            out.append(str(entry.getKey())).append(':').append(str(entry.getValue()));
        }
        return out.append('}').toString();
    }

    // ── binding without Jackson ────────────────────────────────────────────────────────────────

    private static Object bind(Type type, Object json) throws Exception {
        Class<?> raw = raw(type);
        if (json == null) {
            return raw.isPrimitive() ? primitiveDefault(raw) : null;
        }
        if (raw == Object.class) {
            return json;
        }
        if (raw == String.class || raw == CharSequence.class) {
            return json instanceof String ? json : String.valueOf(json);
        }
        if (raw == boolean.class || raw == Boolean.class) {
            return Boolean.valueOf(String.valueOf(json));
        }
        if (raw == char.class || raw == Character.class) {
            String text = String.valueOf(json);
            return text.isEmpty() ? null : Character.valueOf(text.charAt(0));
        }
        Class<?> boxed = boxed(raw);
        if (Number.class.isAssignableFrom(boxed)) {
            return number(boxed, String.valueOf(json));
        }
        if (raw.isEnum()) {
            return constant(raw, String.valueOf(json));
        }
        if (raw.isArray() && json instanceof List) {
            List<?> items = (List<?>) json;
            Object array = Array.newInstance(raw.getComponentType(), items.size());
            for (int i = 0; i < items.size(); i++) {
                Array.set(array, i, bind(raw.getComponentType(), items.get(i)));
            }
            return array;
        }
        if (Collection.class.isAssignableFrom(raw) && json instanceof List) {
            Collection<Object> out = Set.class.isAssignableFrom(raw) ? new LinkedHashSet<Object>() : new ArrayList<Object>();
            for (Object item : (List<?>) json) {
                out.add(bind(argument(type, 0), item));
            }
            return out;
        }
        if (Map.class.isAssignableFrom(raw) && json instanceof Map) {
            Map<Object, Object> out = new LinkedHashMap<Object, Object>();
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) json).entrySet()) {
                out.put(entry.getKey(), bind(argument(type, 1), entry.getValue()));
            }
            return out;
        }
        if (json instanceof String) {
            try {
                Method parse = raw.getMethod("parse", CharSequence.class);
                if (Modifier.isStatic(parse.getModifiers())) {
                    return parse.invoke(null, json);
                }
            } catch (NoSuchMethodException notParseable) {
                // fall through
            }
        }
        if (json instanceof Map) {
            Constructor<?> constructor = raw.getDeclaredConstructor();
            constructor.setAccessible(true);
            Object instance = constructor.newInstance();
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) json).entrySet()) {
                set(instance, String.valueOf(entry.getKey()), entry.getValue());
            }
            return instance;
        }
        throw new IllegalArgumentException("cannot bind " + json + " to " + raw.getName() + " without Jackson");
    }

    private static void set(Object instance, String name, Object json) throws Exception {
        if (name.isEmpty()) {
            return;
        }
        String setter = "set" + Character.toUpperCase(name.charAt(0)) + name.substring(1);
        for (Class<?> k = instance.getClass(); k != null && k != Object.class; k = k.getSuperclass()) {
            for (Method method : k.getDeclaredMethods()) {
                if (method.getName().equals(setter) && method.getParameterTypes().length == 1) {
                    method.setAccessible(true);
                    method.invoke(instance, bind(method.getGenericParameterTypes()[0], json));
                    return;
                }
            }
        }
        for (Class<?> k = instance.getClass(); k != null && k != Object.class; k = k.getSuperclass()) {
            try {
                Field field = k.getDeclaredField(name);
                field.setAccessible(true);
                field.set(instance, bind(field.getGenericType(), json));
                return;
            } catch (NoSuchFieldException next) {
                // look in the superclass
            }
        }
        throw new IllegalArgumentException("no property " + name + " on " + instance.getClass().getName());
    }

    private static Class<?> raw(Type type) {
        if (type instanceof Class) {
            return (Class<?>) type;
        }
        if (type instanceof ParameterizedType) {
            return raw(((ParameterizedType) type).getRawType());
        }
        return Object.class;
    }

    private static Type argument(Type type, int index) {
        if (type instanceof ParameterizedType) {
            Type[] arguments = ((ParameterizedType) type).getActualTypeArguments();
            if (arguments.length > index) {
                return arguments[index];
            }
        }
        return Object.class;
    }

    private static Class<?> boxed(Class<?> type) {
        if (!type.isPrimitive()) {
            return type;
        }
        if (type == int.class) return Integer.class;
        if (type == long.class) return Long.class;
        if (type == short.class) return Short.class;
        if (type == byte.class) return Byte.class;
        if (type == double.class) return Double.class;
        if (type == float.class) return Float.class;
        return type;
    }

    private static Object number(Class<?> type, String text) {
        BigDecimal value = new BigDecimal(text);
        if (type == BigDecimal.class || type == Number.class) return value;
        if (type == BigInteger.class) return value.toBigInteger();
        if (type == Long.class) return Long.valueOf(value.longValue());
        if (type == Short.class) return Short.valueOf(value.shortValue());
        if (type == Byte.class) return Byte.valueOf(value.byteValue());
        if (type == Double.class) return Double.valueOf(value.doubleValue());
        if (type == Float.class) return Float.valueOf(value.floatValue());
        return Integer.valueOf(value.intValue());
    }

    private static Object primitiveDefault(Class<?> type) {
        if (type == boolean.class) return Boolean.FALSE;
        if (type == char.class) return Character.valueOf((char) 0);
        return number(boxed(type), "0");
    }

    // ── what an object holds, as a tree ────────────────────────────────────────────────────────

    private static String tree(String name, Object value, int depth, IdentityHashMap<Object, Boolean> seen) {
        StringBuilder out = new StringBuilder("{");
        if (name != null) {
            out.append("\"name\":").append(str(name)).append(',');
        }
        if (value == null) {
            return out.append("\"type\":null,\"text\":\"null\",\"children\":[]}").toString();
        }
        Class<?> type = value.getClass();
        out.append("\"type\":").append(str(type.isArray() ? type.getComponentType().getSimpleName() + "[]" : type.getSimpleName())).append(',');
        if (isLeaf(type)) {
            return out.append("\"text\":").append(str(truncate(text(value)))).append(",\"children\":[]}").toString();
        }
        if (depth >= 8 || seen.containsKey(value)) {
            return out.append("\"text\":\"…\",\"children\":[]}").toString();
        }
        List<String> children = new ArrayList<String>();
        seen.put(value, Boolean.TRUE);
        if (value instanceof Map) {
            int i = 0;
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) value).entrySet()) {
                if (i++ >= 200) break;
                children.add(tree(String.valueOf(entry.getKey()), entry.getValue(), depth + 1, seen));
            }
        } else if (value instanceof Iterable) {
            int i = 0;
            for (Object item : (Iterable<?>) value) {
                if (i >= 200) break;
                children.add(tree("[" + i + "]", item, depth + 1, seen));
                i++;
            }
        } else if (type.isArray()) {
            int length = Math.min(Array.getLength(value), 200);
            for (int i = 0; i < length; i++) {
                children.add(tree("[" + i + "]", Array.get(value, i), depth + 1, seen));
            }
        } else if (isPlatform(type)) {
            seen.remove(value);
            return out.append("\"text\":").append(str(truncate(text(value)))).append(",\"children\":[]}").toString();
        } else {
            List<Class<?>> hierarchy = new ArrayList<Class<?>>();
            for (Class<?> k = type; k != null && k != Object.class; k = k.getSuperclass()) {
                hierarchy.add(0, k);
            }
            for (Class<?> k : hierarchy) {
                for (Field field : k.getDeclaredFields()) {
                    if (Modifier.isStatic(field.getModifiers()) || field.isSynthetic()) {
                        continue;
                    }
                    try {
                        field.setAccessible(true);
                        children.add(tree(field.getName(), field.get(value), depth + 1, seen));
                    } catch (Throwable unreadable) {
                        children.add("{\"name\":" + str(field.getName()) + ",\"type\":null,\"text\":\"(unreadable)\",\"children\":[]}");
                    }
                }
            }
        }
        seen.remove(value);
        return out.append("\"text\":null,\"children\":[").append(String.join(",", children)).append("]}").toString();
    }

    private static boolean isLeaf(Class<?> type) {
        return type.isPrimitive()
            || Enum.class.isAssignableFrom(type)
            || CharSequence.class.isAssignableFrom(type)
            || Number.class.isAssignableFrom(type)
            || type == Boolean.class
            || type == Character.class
            || type == Class.class
            || type == UUID.class
            || type.getName().startsWith("java.time.")
            || Date.class.isAssignableFrom(type)
            || Calendar.class.isAssignableFrom(type);
    }

    private static boolean isPlatform(Class<?> type) {
        String name = type.getName();
        return name.startsWith("java.") || name.startsWith("javax.") || name.startsWith("jdk.") || name.startsWith("sun.");
    }

    // ── text ───────────────────────────────────────────────────────────────────────────────────

    private static String text(Object value) {
        if (value == null) {
            return "null";
        }
        if (value instanceof Class) {
            return ((Class<?>) value).getName();
        }
        if (value.getClass().isArray()) {
            int length = Array.getLength(value);
            StringBuilder out = new StringBuilder("[");
            for (int i = 0; i < length; i++) {
                if (i > 0) out.append(", ");
                out.append(text(Array.get(value, i)));
            }
            return out.append(']').toString();
        }
        if (value instanceof Enum) {
            return ((Enum<?>) value).name();
        }
        return String.valueOf(value);
    }

    private static String truncate(String text) {
        return text.length() > 2000 ? text.substring(0, 2000) + "…" : text;
    }

    private static String explain(Throwable failure) {
        Throwable cause = failure;
        while ((cause instanceof InvocationTargetException || cause instanceof ExceptionInInitializerError)
            && cause.getCause() != null) {
            cause = cause.getCause();
        }
        String message = cause.getMessage();
        if (message == null) {
            return cause.getClass().getSimpleName();
        }
        int newline = message.indexOf('\n');
        return cause.getClass().getSimpleName() + ": " + (newline < 0 ? message : message.substring(0, newline));
    }

    private static String str(String text) {
        if (text == null) {
            return "null";
        }
        StringBuilder out = new StringBuilder(text.length() + 2).append('"');
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            switch (c) {
                case '"': out.append("\\\""); break;
                case '\\': out.append("\\\\"); break;
                case '\n': out.append("\\n"); break;
                case '\r': out.append("\\r"); break;
                case '\t': out.append("\\t"); break;
                default:
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
            }
        }
        return out.append('"').toString();
    }

    /** Just enough JSON to bind a payload when the project has no Jackson. */
    static final class Json {
        private final String text;
        private int at;

        private Json(String text) {
            this.text = text;
        }

        static Object parse(String text) {
            Json parser = new Json(text);
            parser.space();
            Object value = parser.value();
            parser.space();
            if (parser.at != text.length()) {
                throw parser.error("unexpected text after the value");
            }
            return value;
        }

        private Object value() {
            if (at >= text.length()) {
                throw error("unexpected end");
            }
            char c = text.charAt(at);
            if (c == '{') return object();
            if (c == '[') return array();
            if (c == '"') return string();
            if (text.startsWith("true", at)) { at += 4; return Boolean.TRUE; }
            if (text.startsWith("false", at)) { at += 5; return Boolean.FALSE; }
            if (text.startsWith("null", at)) { at += 4; return null; }
            return number();
        }

        private Map<String, Object> object() {
            at++;
            Map<String, Object> out = new LinkedHashMap<String, Object>();
            space();
            if (peek('}')) { at++; return out; }
            while (true) {
                space();
                String key = string();
                space();
                expect(':');
                space();
                out.put(key, value());
                space();
                if (peek(',')) { at++; continue; }
                expect('}');
                return out;
            }
        }

        private List<Object> array() {
            at++;
            List<Object> out = new ArrayList<Object>();
            space();
            if (peek(']')) { at++; return out; }
            while (true) {
                space();
                out.add(value());
                space();
                if (peek(',')) { at++; continue; }
                expect(']');
                return out;
            }
        }

        private String string() {
            expect('"');
            StringBuilder out = new StringBuilder();
            while (at < text.length()) {
                char c = text.charAt(at++);
                if (c == '"') {
                    return out.toString();
                }
                if (c != '\\') {
                    out.append(c);
                    continue;
                }
                if (at >= text.length()) {
                    break;
                }
                char escaped = text.charAt(at++);
                switch (escaped) {
                    case 'n': out.append('\n'); break;
                    case 't': out.append('\t'); break;
                    case 'r': out.append('\r'); break;
                    case 'b': out.append('\b'); break;
                    case 'f': out.append('\f'); break;
                    case 'u':
                        out.append((char) Integer.parseInt(text.substring(at, at + 4), 16));
                        at += 4;
                        break;
                    default: out.append(escaped);
                }
            }
            throw error("unterminated string");
        }

        private Object number() {
            int start = at;
            while (at < text.length() && "+-0123456789.eE".indexOf(text.charAt(at)) >= 0) {
                at++;
            }
            if (start == at) {
                throw error("unexpected character");
            }
            return new BigDecimal(text.substring(start, at));
        }

        private boolean peek(char c) {
            return at < text.length() && text.charAt(at) == c;
        }

        private void expect(char c) {
            if (!peek(c)) {
                throw error("expected '" + c + "'");
            }
            at++;
        }

        private void space() {
            while (at < text.length() && Character.isWhitespace(text.charAt(at))) {
                at++;
            }
        }

        private IllegalArgumentException error(String what) {
            return new IllegalArgumentException("invalid JSON at character " + at + ": " + what);
        }
    }
}
