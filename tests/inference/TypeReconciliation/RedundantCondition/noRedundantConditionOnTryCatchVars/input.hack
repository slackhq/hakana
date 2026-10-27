function trycatch(): void {
    $value = null;
    try {
        if (rand() % 2 > 0) {
            throw new RuntimeException("Failed");
        }
        $value = new stdClass();
        if (rand() % 2 > 0) {
            throw new RuntimeException("Failed");
        }
    } catch (Exception $e) {
        if ($value !== null) {
            var_export($value);
        }
    }

    if ($value !== null) {}
}