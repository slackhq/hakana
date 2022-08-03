function paradox2(): void {
    $condition = rand() % 2 > 0;

    if (!$condition) {
        foreach (vec[1, 2] as $value) {
            if ($condition) { }
            $condition = true;
        }
    }
}