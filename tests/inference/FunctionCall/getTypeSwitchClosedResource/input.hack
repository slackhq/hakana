$data = "foo";
switch (gettype($data)) {
    case "resource (closed)":
    case "unknown type":
        return "foo";
}