use namespace HH\Lib\{C, Str, Vec};
use type Sprockets\Spindle;

type blorb_t = shape(
	'id' => int,
	?'price' => int,
);

const int SPROCKETS_PER_ACRE_FOOT = 3600;

const keyset<string> SPROCKET_IMAGE_FORMATS = keyset[
	'image/jpeg',
	'image/gif',
];

public async function topLevelFunction(
	Widget $widget,
	\Whatsits\Whatsit $whatsit,
	vec<AccessLevel> $levels,
	opts_item $opt,
	evaluator_opts $opts = shape(),
): Awaitable<bool> {

	$main_whatsit = await $whatsit->fetchMainWhatsit();
	$foo_opt = Shapes::idx($opts, 'foo_opt', false);
	$bar_opt = Shapes::idx($opts, 'bar_opt', false);
	$baz_opt = Shapes::idx($opts, 'baz_opt', false);
	return await \trace('get_results', async $span ==> {

		\function_with_no_arguments();
		\trace_add_tag($span, \TraceTagKey::TYPE, (int)$foo_opt['type']);
		\trace_add_tag($span, \TraceTagKey::WIDGET_ID, $widget->getId());

		// Split based off opt_type
		switch ($foo_opt['type']) {
			case SprocketType::TYPE_A:
				$sprocket_id = Shapes::idx($foo_opt, 'sprocket_id');
				if (!$sprocket_id) {
					throw new \Exception('Need sprocket_id');
				}
				$sprocket = await \Sprockets\SprocketStore::fetchById(
					$whatsit,
					$sprocket_id,
				);
				if (!$sprocket) {
					throw new \Exception('Sprocket not found');
				}

				// Lorem ipsum dolor sit amet, consectetur adipiscing elit.
				// Etiam dignissim finibus libero.
				if (!$bar_opt) {
					$share = await Helper::containsOtherSprocket(
						$widget as \Widgets\Writeable,
						vec[0],
						$levels,
						vec[
							shape(
								'type' => Type::MAIN,
								'id' => $main_whatsit->getId(),
							),
							shape(
								'type' => Type::NORMAL,
								'id' => $whatsit->getId(),
							),
						],
						true,
					);

					if ($condition) {
						return true;
					}
				}

				// Lorem ipsum dolor sit amet, consectetur adipiscing elit.
				$exclude_foo = true;
				$shares = \get_the_stuff($whatsit, $widget->getId(), $sprocket->getId(), false, $exclude_foo);
				if ($shares->is_error()) {
					throw new \Exception('Could not fetch shares');
				}
				$share_rows = $shares->get()['rows'];

				// Lorem ipsum dolor sit amet, consectetur adipiscing elit.
				// Etiam dignissim finibus libero.
				// Lorem ipsum dolor sit amet, consectetur adipiscing elit.
				$widget_sprocket_id = null;
				if ($sprocket->hasExtras()) {
					$res = await \Namespace\StuffService::fetchExtrasForSprocket($sprocket);
					if ($res->is_error()) {
						await \Logger\Log::error(
							'unable to fetch the extras',
							dict['sprocket_id' => $sprocket->getId(), 'whatsit_id' => $whatsit->getId()],
						);
					}
					$embedded_object = $res->get();
					if ($embedded_object is nonnull) {
						$embedded_object_sprocket = await \Namespace\SprocketService::fetch($embedded_object, $main_whatsit);
						if ($embedded_object_sprocket is nonnull) {
							$widget_sprocket_id = $embedded_object_sprocket->getId();

							$widget_sprocket_shares = \get_shares(
								$main_whatsit,
								$widget->getId(),
								$widget_sprocket_id,
								false,
								$exclude_foo,
							);

							if ($widget_sprocket_shares->is_error()) {
								throw new \Exception('Could not fetch widget sprocket shares');
							}

							$widget_share_rows = Vec\concat($widget_share_rows, $widget_sprocket_shares->get()['rows']);
						}
					}
				}

				if (C\is_empty($widget_share_rows)) {
					return false; // Hello!
				}

				// Lorem ipsum to the max.
				$all_ts = Vec\map($widget_share_rows, $foo ==> $foo['ts']);

				$sprocket_opts = vec[
					shape('type' => SprocketType::TYPE_A, 'id' => $sprocket->getId()),
				];
				if ($widget_sprocket_id is nonnull) {
					$sprocket_opts[] = shape(
						'type' => SprocketType::TYPE_A,
						'id' => $sprocket_id,
					);
				}

				return await ClassName::containsInterestingStuff(
					$widget as \Widgets\Writeable,
					$all_ts,
					$levels,
					$sprocket_opts,
					true, // Why not?
				);
		}
		return false;
	});
}

final class WidgetFactory {

	private string $color;

	public function __construct(string $color) {
		$this->color = $color;
	}

	public function getColor(): string {
		return $this->color;
	}
}
