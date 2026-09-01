use namespace Facebook\XHP\Core as x;
use type Facebook\XHP\HTML\{span};

final xhp class foo extends x\element {
	attribute string target = null;
	attribute string other;

	<<__Override>>
	protected async function renderAsync(): Awaitable<x\node> {
		$foo = \HH\Lib\Str\replace($this->:target,'foo', 'bar');
        $foo .= \HH\Lib\Str\replace($this->:other,'foo', 'bar');
		return (<span>$foo</span>);
	}
}
