package verify.finals;

public class FinalFieldInitializedAtDeclaration {
    private final Object value = new Object();

    public FinalFieldInitializedAtDeclaration() {
    }

    public Object value() {
        return value;
    }
}
