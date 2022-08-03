function render(vec_or_dict $data): ?Traversable {
    if ($data["o"] is Traversable) {
        return $data["o"];
    }

    return null;
}