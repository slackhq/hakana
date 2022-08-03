function getString(object $value) : ?string {
    if (method_exists($value, "__toString")) {
        return (string) $value;
    }

    return null;
}