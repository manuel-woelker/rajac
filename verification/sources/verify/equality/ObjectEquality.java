package verify.equality;

public class ObjectEquality {
    public static boolean sameReference(String left, String right) {
        return left == right;
    }

    public static boolean differentReference(String left, String right) {
        return left != right;
    }

    public static boolean equalsMethod(String left, String right) {
        return left.equals(right);
    }

    public static boolean compareWithNull(String value) {
        return value == null;
    }
}
