function array_append(vec<string> $errors): ?vec<string> {
    if (!C\is_empty($errors)) {
        return $errors;
    }
    if (rand() % 2 > 0) {
        $errors[] = "unlucky";
    }
    if (!C\is_empty($errors)) {
        return null;
    }
    return $errors;
}