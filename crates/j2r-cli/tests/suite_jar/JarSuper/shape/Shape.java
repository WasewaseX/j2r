package shape;

public abstract class Shape implements Area {
    protected final String name;

    protected Shape(String name) {
        this.name = name;
    }

    public String describe() {
        return name + "=" + area();
    }
}
