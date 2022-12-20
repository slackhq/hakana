function render(dict<string, mixed> $data): ?Traversable {
    /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
    if ($data["o"] is Traversable) {
        return $data["o"];
    }

    return null;
}