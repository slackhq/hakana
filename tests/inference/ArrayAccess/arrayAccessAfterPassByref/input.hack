final class Arr {
    public static function pull(inout dict<string, mixed> $a, string $b, mixed $c = null): mixed {
        return $a[$b] ?? $c;
    }
}

function _renderButton(dict<string, mixed> $settings): void {
    Arr::pull(inout $settings, "a", true);

    if (isset($settings["b"])) {
        Arr::pull(inout $settings, "b");
    }

    if (isset($settings["c"])) {}
}