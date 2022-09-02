use namespace HH\Lib\Regex;

function foo(string $some_string): void {
    $match = Regex\first_match(
      $some_string,
      re"^(positional)and(?<named>foo)$"
    );

    if ($match is nonnull) {
      $match[0]; // OK, full matched string
      $match[1]; // OK, first positional group
      $match['named']; // OK, named group
      $match['nonexistent']; // ERROR: The field nonexistent is undefined (Typing[4108])
    }
}