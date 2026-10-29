function identity<T>(T $value): T {
    return $value;
}

function collect_args<Targs as (mixed...)>(...Targs $args): Targs {
    return $args;
}

function format_arguments(int $number, string $text): string {
    return HH\Lib\Str\format('%d: %s', $number, $text);
}

function format_nested_arguments(int $number, string $text): string {
    return HH\Lib\Str\format(
        '%d: %s',
        identity($number),
        HH\Lib\Str\format('%s', identity($text)),
    );
}

function no_format_arguments(): string {
    return HH\Lib\Str\format('constant');
}

function unpacked_format_arguments(int $number, string $text): string {
    return HH\Lib\Str\format('%d: %s', ...tuple($number, $text));
}

function partly_unpacked_format_arguments(int $number, string $text): string {
    return HH\Lib\Str\format('%d: %s', $number, ...tuple($text));
}

function fail_with_arguments(int $number, string $text): noreturn {
    invariant_violation('%d: %s', $number, $text);
}

function fail_with_nested_arguments(int $number, string $text): noreturn {
    invariant_violation('%d: %s', identity($number), identity($text));
}

function preserve_tuple(int $number, string $text): (int, string) {
    return collect_args($number, $text);
}

function preserve_nested_tuple(int $number, string $text): (int, string) {
    return collect_args(identity($number), identity($text));
}
