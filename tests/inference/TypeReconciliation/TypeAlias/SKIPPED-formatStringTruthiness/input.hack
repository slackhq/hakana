interface Foo {}

type MyFormatString = HH\FormatString<Foo>;

function bar(HH\FormatString<Foo> $x, MyFormatString $y): void {
  if ($x as string) {}
  if ($y as string) {}
}