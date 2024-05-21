trait T {
    public static function filterInstance(mixed $instance): ?this {
        return $instance is this ? $instance : null;
    }
}

final class A {
    use T;
}