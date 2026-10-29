function conditional(bool $replace): int {
    $value = 'before';
    try {
        $value = 1;
    } finally {
        if ($replace) {
            $value = 'after';
        }
    }
    return $value;
}
