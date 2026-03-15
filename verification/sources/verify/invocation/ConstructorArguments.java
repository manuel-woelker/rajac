package verify.invocation;

class ConstructorArgumentsHelper {
    ConstructorArgumentsHelper(int value) {
    }

    int value() {
        return 9;
    }
}

public class ConstructorArguments {
    public static int build() {
        return new ConstructorArgumentsHelper(9).value();
    }
}
