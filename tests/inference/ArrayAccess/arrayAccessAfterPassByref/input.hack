class Arr {
    public static function pull(inout vec_or_dict $a, string $b, mixed $c = null): mixed {
        return $a[$b] ?? $c;
    }
}

function _renderButton(vec_or_dict $settings): void {
    Arr::pull($settings, "a", true);

    if (isset($settings["b"])) {
        Arr::pull($settings, "b");
    }

    if (isset($settings["c"])) {}
}