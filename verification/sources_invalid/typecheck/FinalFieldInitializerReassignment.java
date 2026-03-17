public class FinalFieldInitializerReassignment {
    private final Object value = new Object();

    public FinalFieldInitializerReassignment() {
        value = new Object();
    }
}
