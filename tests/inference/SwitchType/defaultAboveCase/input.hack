function foo(string $a) : string {
  switch ($a) {
    case "a":
      return "hello";

    default:
    case "b":
      return "goodbye";
  }
}