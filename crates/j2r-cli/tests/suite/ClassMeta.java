public class ClassMeta {
    static class Nested {
        int marker = 1;
    }

    static class Child extends ClassMeta {
    }

    interface Describable {
        String describe();
    }

    public static void main(String[] args) {
        // getName: binary names with dots; primitives as keywords; arrays in
        // descriptor form.
        System.out.println(ClassMeta.class.getName());
        System.out.println(Nested.class.getName());
        System.out.println(int.class.getName());
        System.out.println(void.class.getName());
        System.out.println(String[].class.getName());
        System.out.println(int[].class.getName());
        System.out.println(int[][].class.getName());
        // getSimpleName: InnerClasses attribute for nested, brackets for arrays.
        System.out.println(Nested.class.getSimpleName());
        System.out.println(String[].class.getSimpleName());
        System.out.println(int[][].class.getSimpleName());
        System.out.println(int.class.getSimpleName());
        // toString: class / interface / primitive / array shapes.
        System.out.println(ClassMeta.class);
        System.out.println(Describable.class);
        System.out.println(int.class);
        System.out.println(String[].class);
        System.out.println(String.class);
        // isInterface / isArray / isPrimitive.
        System.out.println(Describable.class.isInterface());
        System.out.println(ClassMeta.class.isInterface());
        System.out.println(int[].class.isArray());
        System.out.println(int.class.isArray());
        System.out.println(int.class.isPrimitive());
        System.out.println(Integer.class.isPrimitive());
        System.out.println(void.class.isPrimitive());
        // getSuperclass: nested classes still extend Object; a real
        // subclass chains up (cross-type comparisons use equals — javac
        // rejects == on captured wildcards).
        System.out.println(Nested.class.getSuperclass().equals(Object.class));
        System.out.println(Child.class.getSuperclass().equals(ClassMeta.class));
        System.out.println(Child.class.getSuperclass().getSuperclass().equals(Object.class));
        System.out.println(Object.class.getSuperclass() == null);
        System.out.println(Describable.class.getSuperclass() == null);
        System.out.println(int.class.getSuperclass() == null);
        System.out.println(int[].class.getSuperclass().equals(Object.class));
        System.out.println(Integer.class.getSuperclass().equals(Number.class));
        // getComponentType.
        System.out.println(int[].class.getComponentType() == int.class);
        System.out.println(String[].class.getComponentType() == String.class);
        System.out.println(ClassMeta.class.getComponentType() == null);
        // isInstance across the hierarchy, on arrays and boxed values.
        Object n = new Nested();
        System.out.println(ClassMeta.class.isInstance(n));
        System.out.println(Nested.class.isInstance(n));
        System.out.println(Nested.class.isInstance(null));
        System.out.println(Object.class.isInstance(new int[] { 1, 2 }));
        System.out.println(Integer.class.isInstance(7));
        System.out.println(String.class.isInstance(n));
        System.out.println(int.class.isInstance(5));
        // isAssignableFrom.
        System.out.println(Object.class.isAssignableFrom(Nested.class));
        System.out.println(ClassMeta.class.isAssignableFrom(Child.class));
        System.out.println(ClassMeta.class.isAssignableFrom(Nested.class));
        System.out.println(Nested.class.isAssignableFrom(ClassMeta.class));
        System.out.println(Describable.class.isAssignableFrom(Nested.class));
        System.out.println(int.class.isAssignableFrom(int.class));
        System.out.println(ClassMeta.class.isInstance(new Child()));
    }
}
