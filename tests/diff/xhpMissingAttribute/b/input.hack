use type Facebook\XHP\Core\frag;

xhp class MyElement extends \Facebook\XHP\Core\element {
    attribute string some-attr @required;

    public final async function renderAsync(): Awaitable<\Facebook\XHP\Core\node> {
		return <frag>{$this->:some-attr}</frag>;
	}
}

function foo(): \XHPChild {
    return <MyElement />;
}