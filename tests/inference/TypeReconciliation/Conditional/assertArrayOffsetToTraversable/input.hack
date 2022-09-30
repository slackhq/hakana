function render(dict<string, mixed> $data): ?Traversable {
    if ($data["o"] is Traversable) {
        return $data["o"];
    }

    return null;
}