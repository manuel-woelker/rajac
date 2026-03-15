package verify.arithmetic;

public class PrimitiveNumberLiterals {
    public static byte minByte() {
        return -128;
    }

    public static byte maxByte() {
        return 127;
    }

    public static short minShort() {
        return -32768;
    }

    public static short maxShort() {
        return 32767;
    }

    public static int minInt() {
        return -2147483648;
    }

    public static int maxInt() {
        return 2147483647;
    }

    public static long minLong() {
        return -9223372036854775808L;
    }

    public static long maxLong() {
        return 9223372036854775807L;
    }

    public static float minFloat() {
        return -3.4028235e38f;
    }

    public static float maxFloat() {
        return 3.4028235e38f;
    }

    public static double minDouble() {
        return -1.7976931348623157e308d;
    }

    public static double maxDouble() {
        return 1.7976931348623157e308d;
    }

    public static char minChar() {
        return '\u0000';
    }

    public static char maxChar() {
        return '\uffff';
    }
}
