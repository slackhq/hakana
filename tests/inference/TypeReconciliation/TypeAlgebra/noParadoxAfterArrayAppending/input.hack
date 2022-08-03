function array_append(vec<string> $errors): ?vec<string> {
    if ($errors) {
        return $errors;
    }
    if (rand() % 2 > 0) {
        $errors[] = "unlucky";
    }
    if ($errors) {
        return null;
    }
    return $errors;
}