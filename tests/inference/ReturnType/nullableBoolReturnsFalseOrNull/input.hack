function is_disabled(bool $known): ?bool {
    return $known ? false : null;
}

function find_position(string $haystack, bool $search): ?int {
    return $search ? HH\Lib\Str\search($haystack, 'a') : false;
}
