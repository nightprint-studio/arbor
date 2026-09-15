#!/usr/bin/env python3
"""Generate the differential corpus the `javac_diff` example scores against.

Tiny Java files, each with exactly ONE intended defect, plus a set that is deliberately
tricky and perfectly legal. Each case becomes its own directory — indexed as an isolated
project, the way the langtools harness treats a jtreg test — and `javac` is run over it to
record what a real compiler says, in `-XDrawDiagnostics` form, as `expected.out`.

One defect per case is the whole point: it is what makes a Bennu diagnostic anywhere ELSE
in that file a false positive by construction, which no corpus of broken code can tell you
and no corpus of working code can either.

    python3 javac_diff_corpus.py /tmp/diffcorpus
    cargo run -p bennu-intel --release --example javac_diff -- /tmp/diffcorpus [detail]

The script reports any case where javac disagrees with its own label — an "error" case that
compiles, or a "clean" case that does not — so the corpus cannot quietly rot as the JDK moves.
"""
import os, sys, subprocess, shutil

CASES = {}

def case(name, **files):
    assert name not in CASES, name
    CASES[name] = files

# ---------------------------------------------------------------- A. resolution
case("res_unknown_var", **{"A.java": """
public class A {
    void m() {
        int y = x + 1;
    }
}
"""})
case("res_unknown_method", **{"A.java": """
public class A {
    void m() {
        nope();
    }
}
"""})
case("res_unknown_type", **{"A.java": """
public class A {
    Zork f;
}
"""})
case("res_unknown_field_on_type", **{"A.java": """
public class A {
    static class B { int a; }
    void m(B b) { int q = b.nothere; }
}
"""})
case("res_unknown_method_on_type", **{"A.java": """
public class A {
    void m(String s) { s.nothere(); }
}
"""})
case("res_bad_import", **{"A.java": """
import java.util.NoSuchClassHere;
public class A { }
"""})
case("res_bad_package_import", **{"A.java": """
import com.nowhere.at.all.Thing;
public class A { }
"""})
case("res_unknown_static_import", **{"A.java": """
import static java.lang.Math.nosuchmethod;
public class A { }
"""})
case("res_var_out_of_scope", **{"A.java": """
public class A {
    void m() {
        { int k = 1; }
        int q = k;
    }
}
"""})
case("res_unknown_in_ctor_call", **{"A.java": """
public class A {
    A(int i) {}
    static A make() { return new A(zzz); }
}
"""})

# ---------------------------------------------------------------- B. types
case("ty_assign_string_to_int", **{"A.java": """
public class A {
    void m() { int x = "hello"; }
}
"""})
case("ty_assign_int_to_string", **{"A.java": """
public class A {
    void m() { String s = 42; }
}
"""})
case("ty_return_mismatch", **{"A.java": """
public class A {
    int m() { return "no"; }
}
"""})
case("ty_return_value_from_void", **{"A.java": """
public class A {
    void m() { return 1; }
}
"""})
case("ty_missing_return", **{"A.java": """
public class A {
    int m() { int x = 1; }
}
"""})
case("ty_lossy_long_to_int", **{"A.java": """
public class A {
    void m() { long l = 5L; int i = l; }
}
"""})
case("ty_lossy_double_to_float", **{"A.java": """
public class A {
    void m() { double d = 1.0; float f = d; }
}
"""})
case("ty_if_not_boolean", **{"A.java": """
public class A {
    void m() { int x = 1; if (x) { } }
}
"""})
case("ty_while_not_boolean", **{"A.java": """
public class A {
    void m() { String s = "a"; while (s) { } }
}
"""})
case("ty_null_to_primitive", **{"A.java": """
public class A {
    void m() { int x = null; }
}
"""})
case("ty_object_to_string_no_cast", **{"A.java": """
public class A {
    void m(Object o) { String s = o; }
}
"""})
case("ty_array_required", **{"A.java": """
public class A {
    void m(String s) { char c = s[0]; }
}
"""})
case("ty_string_length_field", **{"A.java": """
public class A {
    void m(String s) { int n = s.length; }
}
"""})
case("ty_array_length_method", **{"A.java": """
public class A {
    void m(int[] a) { int n = a.length(); }
}
"""})
case("ty_deref_primitive", **{"A.java": """
public class A {
    void m(int i) { i.toString(); }
}
"""})
case("ty_bad_binary_operands", **{"A.java": """
public class A {
    void m(boolean b) { int x = b - 1; }
}
"""})
case("ty_inconvertible_cast", **{"A.java": """
public class A {
    void m(String s) { Integer i = (Integer) s; }
}
"""})
case("ty_array_init_mismatch", **{"A.java": """
public class A {
    int[] a = { 1, "two", 3 };
}
"""})
case("ty_ternary_assign_mismatch", **{"A.java": """
public class A {
    void m(boolean b) { int x = b ? "a" : "b"; }
}
"""})
case("ty_unary_not_on_int", **{"A.java": """
public class A {
    void m(int i) { boolean b = !i; }
}
"""})
case("ty_foreach_wrong_element", **{"A.java": """
import java.util.List;
public class A {
    void m(List<String> l) { for (Integer i : l) { } }
}
"""})
case("ty_foreach_not_iterable", **{"A.java": """
public class A {
    void m(String s) { for (char c : s) { } }
}
"""})

# ---------------------------------------------------------------- C. applicability
case("app_too_many_args", **{"A.java": """
public class A {
    void f(int a) {}
    void m() { f(1, 2); }
}
"""})
case("app_too_few_args", **{"A.java": """
public class A {
    void f(int a, int b) {}
    void m() { f(1); }
}
"""})
case("app_wrong_arg_type", **{"A.java": """
public class A {
    void f(int a) {}
    void m() { f("s"); }
}
"""})
case("app_ctor_wrong_args", **{"A.java": """
public class A {
    A(int a) {}
    static void m() { new A("s"); }
}
"""})
case("app_no_default_ctor", **{"A.java": """
public class A {
    A(int a) {}
    static void m() { new A(); }
}
"""})
case("app_instance_from_static", **{"A.java": """
public class A {
    void f() {}
    static void m() { f(); }
}
"""})
case("app_this_in_static", **{"A.java": """
public class A {
    int v;
    static void m() { int x = this.v; }
}
"""})
case("app_instance_field_from_static", **{"A.java": """
public class A {
    int v;
    static void m() { int x = v; }
}
"""})
case("app_ambiguous", **{"A.java": """
public class A {
    void f(Object o, String s) {}
    void f(String s, Object o) {}
    void m() { f("a", "b"); }
}
"""})
case("app_varargs_wrong_type", **{"A.java": """
public class A {
    void f(int... a) {}
    void m() { f("a"); }
}
"""})
case("app_generic_method_bad_arg", **{"A.java": """
import java.util.List;
import java.util.ArrayList;
public class A {
    void m() { List<String> l = new ArrayList<>(); l.add(1); }
}
"""})

# ---------------------------------------------------------------- D. access / finals
case("acc_private_field", **{"A.java": """
public class A {
    void m(B b) { int x = b.hidden; }
}
class B { private int hidden; }
"""})
case("acc_private_method", **{"A.java": """
public class A {
    void m(B b) { b.hidden(); }
}
class B { private void hidden() {} }
"""})
case("acc_private_ctor", **{"A.java": """
public class A {
    void m() { new B(); }
}
class B { private B() {} }
"""})
case("fin_assign_final_local", **{"A.java": """
public class A {
    void m() { final int x = 1; x = 2; }
}
"""})
case("fin_assign_final_field", **{"A.java": """
public class A {
    final int x = 1;
    void m() { x = 2; }
}
"""})
case("fin_blank_final_twice", **{"A.java": """
public class A {
    final int x;
    A() { x = 1; x = 2; }
}
"""})
case("fin_blank_final_unset", **{"A.java": """
public class A {
    final int x;
    A() { }
}
"""})
case("fin_assign_to_param_of_lambda", **{"A.java": """
public class A {
    void m() {
        int c = 0;
        Runnable r = () -> System.out.println(c);
        c = 5;
    }
}
"""})

# ---------------------------------------------------------------- E. inheritance
case("inh_extend_final", **{"A.java": """
public class A extends B { }
final class B { }
"""})
case("inh_abstract_not_implemented", **{"A.java": """
public class A extends B { }
abstract class B { abstract void f(); }
"""})
case("inh_iface_not_implemented", **{"A.java": """
public class A implements I { }
interface I { void f(); }
"""})
case("inh_override_weaker_access", **{"A.java": """
public class A extends B { private void f() {} }
class B { public void f() {} }
"""})
case("inh_override_bad_return", **{"A.java": """
public class A extends B { int f() { return 1; } }
class B { String f() { return null; } }
"""})
case("inh_override_final", **{"A.java": """
public class A extends B { void f() {} }
class B { final void f() {} }
"""})
case("inh_abstract_method_in_concrete", **{"A.java": """
public class A { abstract void f(); }
"""})
case("inh_extend_interface_with_class", **{"A.java": """
public class A extends I { }
interface I { }
"""})
case("inh_implement_class", **{"A.java": """
public class A implements B { }
class B { }
"""})
case("inh_cycle", **{"A.java": """
public class A extends B { }
class B extends A { }
"""})
case("inh_instantiate_abstract", **{"A.java": """
public class A {
    void m() { new B(); }
}
abstract class B { }
"""})
case("inh_instantiate_interface", **{"A.java": """
public class A {
    void m() { new I(); }
}
interface I { }
"""})
case("inh_super_no_default_ctor", **{"A.java": """
public class A extends B { A() { } }
class B { B(int x) {} }
"""})
case("inh_override_annotation_wrong", **{"A.java": """
public class A extends B { @Override void g() {} }
class B { void f() {} }
"""})
case("inh_extend_multiple_ifaces_clash", **{"A.java": """
public class A implements I, J { public int f() { return 1; } }
interface I { int f(); }
interface J { String f(); }
"""})

# ---------------------------------------------------------------- F. constructors
case("ctor_this_not_first", **{"A.java": """
public class A {
    A() { int x = 1; this(2); }
    A(int i) {}
}
"""})
case("ctor_recursive", **{"A.java": """
public class A {
    A() { this(); }
}
"""})
case("ctor_return_type", **{"A.java": """
public class A {
    int A() { return 1; }
    void m() { new A(1); }
}
"""})
case("ctor_super_after_stmt", **{"A.java": """
public class A extends B {
    A() { int x = 1; super(x); }
}
class B { B(int i) {} }
"""})

# ---------------------------------------------------------------- G. exceptions
case("exc_unreported_checked", **{"A.java": """
import java.io.IOException;
public class A {
    void f() throws IOException {}
    void m() { f(); }
}
"""})
case("exc_unreported_new", **{"A.java": """
import java.io.IOException;
public class A {
    void m() { throw new IOException(); }
}
"""})
case("exc_catch_never_thrown", **{"A.java": """
import java.io.IOException;
public class A {
    void m() { try { int x = 1; } catch (IOException e) { } }
}
"""})
case("exc_dead_catch_order", **{"A.java": """
public class A {
    void m() {
        try { int x = 1; }
        catch (RuntimeException e) { }
        catch (IllegalStateException e) { }
    }
}
"""})
case("exc_override_widens_throws", **{"A.java": """
import java.io.IOException;
public class A extends B { void f() throws IOException {} }
class B { void f() {} }
"""})
case("exc_throw_non_throwable", **{"A.java": """
public class A {
    void m() { throw new String("x"); }
}
"""})
case("exc_catch_non_throwable", **{"A.java": """
public class A {
    void m() { try { } catch (String e) { } }
}
"""})
case("exc_twr_not_autocloseable", **{"A.java": """
public class A {
    void m() { try (String s = "a") { } }
}
"""})

# ---------------------------------------------------------------- H. flow / definite assignment
case("flow_uninitialized_local", **{"A.java": """
public class A {
    void m() { int x; int y = x + 1; }
}
"""})
case("flow_unreachable_after_return", **{"A.java": """
public class A {
    void m() { return; int x = 1; }
}
"""})
case("flow_unreachable_after_throw", **{"A.java": """
public class A {
    void m() { throw new RuntimeException(); }
    void n() { throw new RuntimeException(); int x = 1; }
}
"""})
case("flow_unreachable_in_while_false", **{"A.java": """
public class A {
    void m() { while (false) { int x = 1; } }
}
"""})
case("flow_break_outside_loop", **{"A.java": """
public class A {
    void m() { break; }
}
"""})
case("flow_continue_outside_loop", **{"A.java": """
public class A {
    void m() { continue; }
}
"""})
case("flow_missing_return_branch", **{"A.java": """
public class A {
    int m(boolean b) { if (b) { return 1; } }
}
"""})
case("flow_dup_local", **{"A.java": """
public class A {
    void m() { int x = 1; int x = 2; }
}
"""})
case("flow_dup_param", **{"A.java": """
public class A {
    void m(int a, int a) { }
}
"""})
case("flow_local_shadows_param", **{"A.java": """
public class A {
    void m(int a) { int a = 1; }
}
"""})

# ---------------------------------------------------------------- I. generics
case("gen_type_arg_arity", **{"A.java": """
import java.util.Map;
public class A { Map<String> m; }
"""})
case("gen_bound_violation", **{"A.java": """
public class A<T extends Number> { }
class B { A<String> f; }
"""})
case("gen_assign_generic_mismatch", **{"A.java": """
import java.util.List;
import java.util.ArrayList;
public class A {
    void m() { List<String> l = new ArrayList<Integer>(); }
}
"""})
case("gen_generic_return_mismatch", **{"A.java": """
import java.util.List;
public class A {
    void m(List<String> l) { Integer i = l.get(0); }
}
"""})
case("gen_type_param_on_static", **{"A.java": """
public class A<T> {
    static T f;
}
"""})
case("gen_wildcard_add", **{"A.java": """
import java.util.List;
public class A {
    void m(List<? extends Number> l) { l.add(1); }
}
"""})
case("gen_typearg_on_nongeneric", **{"A.java": """
public class A { String<Integer> s; }
"""})

# ---------------------------------------------------------------- J. switch
case("sw_dup_case", **{"A.java": """
public class A {
    void m(int i) { switch (i) { case 1: break; case 1: break; } }
}
"""})
case("sw_case_type_mismatch", **{"A.java": """
public class A {
    void m(int i) { switch (i) { case "a": break; } }
}
"""})
case("sw_unknown_enum_const", **{"A.java": """
public class A {
    enum E { X, Y }
    void m(E e) { switch (e) { case Z: break; } }
}
"""})
case("sw_qualified_enum_label", **{"A.java": """
public class A {
    enum E { X, Y }
    void m(E e) { switch (e) { case E.X: break; } }
}
"""})
case("sw_dup_default", **{"A.java": """
public class A {
    void m(int i) { switch (i) { default: break; default: break; } }
}
"""})
case("sw_non_constant_label", **{"A.java": """
public class A {
    void m(int i, int j) { switch (i) { case j: break; } }
}
"""})
case("sw_switch_on_bad_type", **{"A.java": """
public class A {
    void m(double d) { switch (d) { } }
}
"""})
case("sw_expr_not_exhaustive", **{"A.java": """
public class A {
    enum E { X, Y }
    int m(E e) { return switch (e) { case X -> 1; }; }
}
"""})

# ---------------------------------------------------------------- K. modifiers / declarations
case("mod_abstract_with_body", **{"A.java": """
public abstract class A { abstract void f() { } }
"""})
case("mod_final_abstract_class", **{"A.java": """
public final abstract class A { }
"""})
case("mod_private_iface_member", **{"A.java": """
public interface A { protected void f(); }
"""})
case("mod_dup_modifier", **{"A.java": """
public class A { public public void f() {} }
"""})
case("mod_static_in_inner", **{"A.java": """
public class A { class B { static int x = 1; static void f() {} } }
"""})
case("mod_dup_method", **{"A.java": """
public class A {
    void f(int a) {}
    void f(int b) {}
}
"""})
case("mod_dup_field", **{"A.java": """
public class A { int x; int x; }
"""})
case("mod_dup_class", **{"A.java": """
public class A { }
class B { }
class B { }
"""})
case("mod_public_class_wrong_file", **{"B.java": """
public class NotB { }
"""})
case("mod_two_public_classes", **{"A.java": """
public class A { }
public class C { }
"""})
case("mod_interface_field_not_final", **{"A.java": """
public interface A { int x = 1; }
class Z { void m() { A.x = 2; } }
"""})
case("mod_enum_extends", **{"A.java": """
public class A { }
enum E extends A { X }
"""})
case("mod_native_with_body", **{"A.java": """
public class A { native void f() { } }
"""})
case("mod_var_as_field", **{"A.java": """
public class A { var x = 1; }
"""})
case("mod_var_no_init", **{"A.java": """
public class A { void m() { var x; } }
"""})

# ---------------------------------------------------------------- L. lambdas / functional
case("lam_not_functional_iface", **{"A.java": """
public class A {
    interface I { void f(); void g(); }
    void m() { I i = () -> {}; }
}
"""})
case("lam_wrong_arity", **{"A.java": """
import java.util.function.Function;
public class A {
    void m() { Function<String, String> f = (a, b) -> a; }
}
"""})
case("lam_capture_non_final", **{"A.java": """
public class A {
    void m() {
        int c = 0;
        c++;
        Runnable r = () -> System.out.println(c);
    }
}
"""})
case("lam_methodref_unknown", **{"A.java": """
public class A {
    void m() { Runnable r = A::nosuch; }
}
"""})
case("lam_return_mismatch", **{"A.java": """
import java.util.function.Supplier;
public class A {
    void m() { Supplier<String> s = () -> 1; }
}
"""})
case("lam_target_not_iface", **{"A.java": """
public class A {
    void m() { String s = () -> {}; }
}
"""})
case("lam_anon_missing_impl", **{"A.java": """
public class A {
    interface I { void f(); }
    void m() { I i = new I() { }; }
}
"""})

# ---------------------------------------------------------------- M. enum / record / annotations
case("enum_new_instance", **{"A.java": """
public class A {
    enum E { X }
    void m() { E e = new E(); }
}
"""})
case("enum_ctor_public", **{"A.java": """
public class A {
    enum E { X; public E() {} }
}
"""})
case("rec_component_assign", **{"A.java": """
public record A(int x) {
    void m() { x = 1; }
}
"""})
case("rec_extends", **{"A.java": """
public record A(int x) extends Object { }
"""})
case("ann_missing_element", **{"A.java": """
public class A {
    @interface Ann { String value(); }
    @Ann static class B { }
}
"""})
case("ann_unknown_element", **{"A.java": """
public class A {
    @interface Ann { String value(); }
    @Ann(nope = "x") static class B { }
}
"""})
case("ann_not_applicable", **{"A.java": """
import java.lang.annotation.*;
public class A {
    @Target(ElementType.METHOD) @interface M { }
    @M int field;
}
"""})
case("ann_non_constant_value", **{"A.java": """
public class A {
    @interface Ann { String value(); }
    static String s = "x";
    @Ann(s) static class B { }
}
"""})
case("ann_wrong_value_type", **{"A.java": """
public class A {
    @interface Ann { int value(); }
    @Ann("x") static class B { }
}
"""})
case("ann_override_on_non_override", **{"A.java": """
public class A { @Override void f() {} }
"""})
case("ann_functional_iface_invalid", **{"A.java": """
@FunctionalInterface
public interface A { void f(); void g(); }
"""})

# ---------------------------------------------------------------- N. misc
case("misc_static_via_instance", **{"A.java": """
public class A {
    static void f() {}
    void m(A a) { a.f(); }
}
"""})
case("misc_package_mismatch", **{"com/acme/A.java": """
package wrong.pkg;
public class A { }
"""})
case("misc_self_assign_field", **{"A.java": """
public class A {
    int x;
    void set(int x) { x = x; }
}
"""})
case("misc_int_div_by_zero", **{"A.java": """
public class A { int x = 1 / 0; }
"""})
case("misc_new_on_primitive", **{"A.java": """
public class A { void m() { int i = new int(); } }
"""})
case("misc_assign_to_method_call", **{"A.java": """
public class A {
    int f() { return 1; }
    void m() { f() = 2; }
}
"""})
case("misc_labeled_break_unknown", **{"A.java": """
public class A {
    void m() { for (;;) { break nope; } }
}
"""})
case("misc_string_switch_null_case", **{"A.java": """
public class A {
    void m(String s) { switch (s) { case null: break; } }
}
"""})
case("misc_instanceof_inconvertible", **{"A.java": """
public class A {
    void m(String s) { if (s instanceof Integer) { } }
}
"""})
case("misc_array_dim_mismatch", **{"A.java": """
public class A { void m() { int[] a = new int[2][2]; } }
"""})

# ============================================================ CLEAN (no errors)
CLEAN = {}
def clean(name, **files):
    assert name not in CASES, name
    CASES[name] = files
    CLEAN[name] = True

clean("ok_generics_basic", **{"A.java": """
import java.util.*;
public class A {
    Map<String, List<Integer>> m = new HashMap<>();
    <T extends Comparable<T>> T max(List<T> xs) {
        T best = xs.get(0);
        for (T x : xs) if (x.compareTo(best) > 0) best = x;
        return best;
    }
}
"""})
clean("ok_lambdas", **{"A.java": """
import java.util.*;
import java.util.function.*;
public class A {
    void m() {
        List<String> l = new ArrayList<>();
        l.sort(Comparator.comparing(String::length));
        Function<String, Integer> f = String::length;
        BiFunction<Integer, Integer, Integer> g = (a, b) -> a + b;
        Supplier<List<String>> s = ArrayList::new;
        Runnable r = () -> { int x = 1; System.out.println(x + f.apply("a") + g.apply(1, 2)); };
        r.run();
        System.out.println(s.get());
    }
}
"""})
clean("ok_inner_anonymous", **{"A.java": """
public class A {
    interface I { int f(int x); }
    class Inner { int v = 3; }
    static class Nested { static int s = 4; }
    int use() {
        I i = new I() { public int f(int x) { return x + new A().new Inner().v + Nested.s; } };
        return i.f(1);
    }
}
"""})
clean("ok_inheritance", **{"A.java": """
public class A extends B implements I {
    @Override public String name() { return "a"; }
    @Override protected int val() { return super.val() + 1; }
}
abstract class B { protected int val() { return 1; } }
interface I { String name(); }
"""})
clean("ok_exceptions", **{"A.java": """
import java.io.*;
public class A {
    void m() {
        try (BufferedReader r = new BufferedReader(new StringReader("x"))) {
            System.out.println(r.readLine());
        } catch (IOException | RuntimeException e) {
            e.printStackTrace();
        } finally {
            System.out.println("done");
        }
    }
    void n() throws Exception { throw new IllegalStateException(); }
}
"""})
clean("ok_switch_modern", **{"A.java": """
public class A {
    enum E { X, Y }
    int m(E e) {
        return switch (e) { case X -> 1; case Y -> 2; };
    }
    String n(int i) {
        switch (i) {
            case 1: return "one";
            case 2:
            case 3: return "few";
            default: return "many";
        }
    }
}
"""})
clean("ok_varargs_overloads", **{"A.java": """
public class A {
    void f(int a) {}
    void f(int a, int b) {}
    void f(String s, Object... rest) {}
    void m() { f(1); f(1, 2); f("a"); f("a", 1, 2); }
}
"""})
clean("ok_static_init", **{"A.java": """
public class A {
    static final int X;
    final int y;
    static { X = 1; }
    { y = 2; }
    A() { }
}
"""})
clean("ok_records_sealed", **{"A.java": """
public class A {
    sealed interface Shape permits Circle, Square { }
    record Circle(double r) implements Shape { }
    record Square(double s) implements Shape { }
    double area(Shape sh) {
        return switch (sh) {
            case Circle c -> 3.14 * c.r() * c.r();
            case Square s -> s.s() * s.s();
        };
    }
}
"""})
clean("ok_boxing_widening", **{"A.java": """
import java.util.*;
public class A {
    void m() {
        Integer i = 1;
        int j = i;
        long l = j;
        double d = l;
        Object o = d;
        List<Integer> xs = new ArrayList<>();
        xs.add(5);
        int k = xs.get(0);
        char c = 'a';
        int ci = c;
        byte b = 3;
        short sh = b;
        System.out.println(o + "" + k + ci + sh);
    }
}
"""})
clean("ok_labels_loops", **{"A.java": """
public class A {
    int m() {
        outer:
        for (int i = 0; i < 3; i++) {
            for (int j = 0; j < 3; j++) {
                if (j == 1) continue outer;
                if (i == 2) break outer;
            }
        }
        int k = 0;
        do { k++; } while (k < 3);
        return k;
    }
}
"""})
clean("ok_interfaces_default", **{"A.java": """
public class A implements I {
    public int f() { return 1; }
    void use() { System.out.println(g() + I.C); }
}
interface I {
    int C = 7;
    int f();
    default int g() { return f() + 1; }
    static int h() { return 2; }
}
"""})
clean("ok_ternary_and_casts", **{"A.java": """
public class A {
    Object m(boolean b, String s, Integer i) {
        Object o = b ? s : i;
        Number n = (Number) i;
        CharSequence cs = s;
        return b ? o : n.intValue() + cs.length();
    }
}
"""})
clean("ok_arrays", **{"A.java": """
public class A {
    void m() {
        int[] a = new int[3];
        int[][] b = new int[2][3];
        String[] s = { "x", "y" };
        int[] c = {1, 2, 3};
        for (int x : c) System.out.println(x + a.length + b[0][1] + s[0]);
    }
}
"""})
clean("ok_generic_class", **{"A.java": """
public class A<T extends Number & Comparable<T>> {
    private final T value;
    A(T value) { this.value = value; }
    T get() { return value; }
    <R> R map(java.util.function.Function<? super T, ? extends R> f) { return f.apply(value); }
    static <U> A<Integer> of() { return new A<>(1); }
}
"""})
clean("ok_enum_rich", **{"A.java": """
public class A {
    enum Op {
        ADD("+") { int apply(int a, int b) { return a + b; } },
        SUB("-") { int apply(int a, int b) { return a - b; } };
        private final String sym;
        Op(String sym) { this.sym = sym; }
        abstract int apply(int a, int b);
        String sym() { return sym; }
    }
    int m() { return Op.ADD.apply(1, 2) + Op.valueOf("SUB").apply(3, 1) + Op.values().length; }
}
"""})
clean("ok_overload_resolution", **{"A.java": """
public class A {
    void f(Object o) {}
    void f(String s) {}
    void f(Integer i) {}
    void f(int i) {}
    void m() { f("x"); f(1); f(Integer.valueOf(2)); f(new Object()); f((Object) "x"); }
}
"""})
clean("ok_inner_generics_shadow", **{"A.java": """
import java.util.*;
public class A<T> {
    class B<T> { T v; }
    <T> void m(T t) { List<T> l = new ArrayList<>(); l.add(t); }
    void n() { B<String> b = new B<>(); b.v = "x"; m(1); }
}
"""})
clean("ok_static_nested_access", **{"A.java": """
public class A {
    private static int counter = 0;
    private int inst = 0;
    static class N { void bump() { counter++; } }
    class In { void bump() { inst++; counter++; } }
    void m() { new N().bump(); new In().bump(); }
}
"""})
clean("ok_string_ops", **{"A.java": """
public class A {
    void m() {
        String s = "a" + 1 + 'c' + 2.0 + true + null;
        int n = s.length();
        char c = s.charAt(0);
        String t = String.format("%s %d", s, n);
        System.out.println(t + c + s.substring(1).toUpperCase().trim());
    }
}
"""})

# Written as errors, and javac 21 accepts them: a qualified enum label (`case E.X`) and a `static`
# member of an inner class became legal in 21 and 16, a call to a static method through an instance
# and a `x = x` self-assignment are warnings at most, a package that does not match its directory is
# not checked without a sourcepath, and `1 / 0` is a constant expression javac is happy to fold.
# They stay in the corpus with the CLEAN set: each is a shape a validation can easily over-report.
for _legal in (
    "sw_qualified_enum_label",
    "mod_static_in_inner",
    "misc_static_via_instance",
    "misc_package_mismatch",
    "misc_self_assign_field",
    "misc_int_div_by_zero",
):
    CLEAN[_legal] = True

# ============================================================ driver
def main():
    out = sys.argv[1]
    if os.path.isdir(out):
        shutil.rmtree(out)
    os.makedirs(out)
    n_err = n_clean = 0
    for name, files in CASES.items():
        d = os.path.join(out, name)
        os.makedirs(d, exist_ok=True)
        rels = []
        for rel, text in files.items():
            p = os.path.join(d, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w") as f:
                f.write(text.lstrip("\n"))
            rels.append(rel)
        classes = os.path.join(d, "_classes")
        os.makedirs(classes, exist_ok=True)
        r = subprocess.run(
            ["javac", "-XDrawDiagnostics", "-Xlint:all", "-d", "_classes", "-nowarn"] + sorted(rels),
            cwd=d, capture_output=True, text=True)
        golden = r.stderr
        with open(os.path.join(d, "expected.out"), "w") as f:
            f.write(golden)
        shutil.rmtree(classes)
        has_err = "compiler.err." in golden
        if name in CLEAN:
            n_clean += 1
            if has_err:
                print(f"!! CLEAN case {name} does not compile:\n{golden}")
        else:
            n_err += 1
            if not has_err:
                print(f"!! ERROR case {name} compiled fine (javac saw nothing)")
    print(f"cases: {len(CASES)}  ({n_err} error, {n_clean} clean)")

if __name__ == "__main__":
    main()
