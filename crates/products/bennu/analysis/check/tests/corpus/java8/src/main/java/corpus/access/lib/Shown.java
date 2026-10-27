package corpus.access.lib;

public class Shown {
    public int open;
    int packageField;
    protected int guarded;
    private int secret;

    public void visible() { }
    void packageMethod() { }
    protected void protectedMethod() { }
}

class Hidden {
    public void method() { }
}
