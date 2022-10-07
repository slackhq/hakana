class Arr {
    public static function pull(inout dict<string, mixed> $a, string $b, mixed $c = null): mixed {
        return $a[$b] ?? $c;
    }
}

function _renderButton(dict<string, mixed> $settings): void {
    Arr::pull($settings, "a", true);

    if (isset($settings["b"])) {
        Arr::pull($settings, "b");
    }

    if (isset($settings["c"])) {}
}