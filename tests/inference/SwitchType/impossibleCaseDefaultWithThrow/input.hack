$a = rand(0, 1) ? "a" : "b";

switch ($a) {
    case "a":
        break;

    case "b":
        break;

    default:
        throw new \Exception("should never happen");
}