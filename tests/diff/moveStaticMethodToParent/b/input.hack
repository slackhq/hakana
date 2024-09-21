abstract class AParent {
    public static function toBeChanged(): void {}
}

final class A extends AParent {
    public static function foo(): void {}
}
