function foo(): int {
    do {
        $value = rand(0, 10);
        if ($value > 5) {
            continue;
        } else {
            break;
        }
    } while (true);

    return $value;
}