package verify.controlflow;

public class IfElse {
    public static String choose(String value) {
        if (value == "foo") {
            return "bar";
        } else {
            return "baz";
        }
    }
}
