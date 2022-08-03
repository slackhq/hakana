interface I {}

abstract class C<T>
{
    protected dict<string, T> $items = dict[];

    // added to trigger constructor initialisation checks
    // in descendant classes
    public int $i;

    public function __construct(dict<string, T> $items = dict[]) {
        $this->i = 5;

        foreach ($items as $k => $v) {
            $this->items[$k] = $v;
        }
    }
}

class Impl implements I {}

class Test extends C<Impl> {}