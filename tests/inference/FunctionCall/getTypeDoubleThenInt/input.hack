function safe_float(mixed $val): bool {
    switch (gettype($val)) {
        case "double":
        case "integer":
            return true;
        // ... more cases omitted
        default:
            return false;
    }
}