use namespace HH\Lib\Str;

function string_arg(string $s): bool {
    return Str\is_empty($s);
}

function nullable_arg(?string $s): bool {
    return Str\is_empty($s);
}
