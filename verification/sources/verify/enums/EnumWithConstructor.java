package verify.enums;

public enum EnumWithConstructor {
    LOW(1),
    HIGH(2);

    private final int code;

    private EnumWithConstructor(int code) {
        this.code = code;
    }

    public int code() {
        return code;
    }
}
