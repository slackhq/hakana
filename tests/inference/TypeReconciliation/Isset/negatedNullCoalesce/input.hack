class Bar {}

type foo_t = shape(
  ?'a' => Bar,
  ?'b' => Bar,
);

function foo(?foo_t $foo): void {
    if ($foo is nonnull && !($foo['a'] ?? null)) {
    } else {
    	if ($foo is nonnull && ($foo['b'] ?? false)) {
        	
    	}
    }
}