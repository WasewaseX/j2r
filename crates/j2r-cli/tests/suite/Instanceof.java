public class Instanceof {
    static class Animal {
        String name() { return "animal"; }
    }
    static class Dog extends Animal implements Pet {
        public String name() { return "dog"; }
        public String pet() { return "wool"; }
    }
    static class Cat extends Animal {
        public String name() { return "cat"; }
    }
    interface Pet {
        String pet();
    }
    public static void main(String[] args) {
        Animal a = new Dog();
        Animal c = new Cat();
        Animal n = null;
        System.out.println(a instanceof Dog);
        System.out.println(a instanceof Cat);
        System.out.println(a instanceof Pet);
        System.out.println(c instanceof Pet);
        System.out.println(n instanceof Animal);
        System.out.println(((Dog) a).pet());
        System.out.println(((Animal) c).name());
        Pet p = (Pet) a;
        System.out.println(p.pet());
        Object[] zoo = { new Dog(), new Cat(), "plain", Integer.valueOf(7) };
        for (Object o : zoo) {
            if (o instanceof Dog) {
                System.out.println("dog:" + ((Dog) o).name());
            } else if (o instanceof Cat) {
                System.out.println("cat:" + ((Cat) o).name());
            } else if (o instanceof String) {
                System.out.println("str:" + o);
            } else {
                System.out.println("other:" + o);
            }
        }
    }
}
