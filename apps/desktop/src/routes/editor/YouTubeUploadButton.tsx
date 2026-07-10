import { Button } from "@cap/ui-solid";
import { createMutation } from "@tanstack/solid-query";
import { Channel } from "@tauri-apps/api/core";
import { createResource, createSignal, Match, Show, Switch } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import toast from "solid-toast";
import Tooltip from "~/components/Tooltip";
import { exportVideo } from "~/utils/export";
import { commands, type UploadProgress } from "~/utils/tauri";
import { useEditorContext } from "./context";
import { Dialog, DialogContent } from "./ui";

const RESOLUTION_1080P = { x: 1920, y: 1080 };

const YouTubeGlyph = (props: { class?: string }) => (
	<svg
		class={props.class}
		viewBox="0 0 24 24"
		xmlns="http://www.w3.org/2000/svg"
		aria-hidden="true"
	>
		<path
			d="M23.5 6.2a3 3 0 0 0-2.11-2.12C19.5 3.55 12 3.55 12 3.55s-7.5 0-9.39.53A3 3 0 0 0 .5 6.2 31.4 31.4 0 0 0 0 12a31.4 31.4 0 0 0 .5 5.8 3 3 0 0 0 2.11 2.12c1.89.53 9.39.53 9.39.53s7.5 0 9.39-.53a3 3 0 0 0 2.11-2.12A31.4 31.4 0 0 0 24 12a31.4 31.4 0 0 0-.5-5.8Z"
			fill="#FF0000"
		/>
		<path d="M9.6 15.6V8.4l6.2 3.6-6.2 3.6Z" fill="#fff" />
	</svg>
);

function formatError(error: unknown): string {
	if (error && typeof error === "object" && "type" in error) {
		const e = error as { type: string; message?: unknown };
		if (typeof e.message === "string") return e.message;
		if (e.message && typeof e.message === "object" && "message" in e.message)
			return String((e.message as { message: unknown }).message);
		return e.type;
	}
	return error instanceof Error ? error.message : "Failed to upload to YouTube";
}

type UploadState =
	| { type: "idle" }
	| { type: "preparing"; rendered: number; total: number }
	| { type: "uploading"; progress: number }
	| { type: "done"; url: string }
	| { type: "error"; message: string };

function YouTubeUploadButton() {
	const { editorInstance } = useEditorContext();
	const projectPath = editorInstance.path;

	const [status] = createResource(() => commands.youtubeGetStatus());

	const [state, setState] = createStore<UploadState>({ type: "idle" });
	const [copied, setCopied] = createSignal(false);

	const percent = () => {
		if (state.type === "preparing")
			return state.total ? Math.round((state.rendered / state.total) * 100) : 0;
		if (state.type === "uploading") return state.progress;
		if (state.type === "done") return 100;
		return 0;
	};

	const inFlight = () =>
		state.type === "preparing" || state.type === "uploading";
	const doneUrl = () => (state.type === "done" ? state.url : "");
	const errorMessage = () => (state.type === "error" ? state.message : "");

	const upload = createMutation(() => ({
		mutationFn: async () => {
			if (!navigator.onLine)
				throw new Error("You appear to be offline. Check your connection.");

			setState(reconcile({ type: "preparing", rendered: 0, total: 0 }));

			await exportVideo(
				projectPath,
				{
					format: "Mp4",
					fps: 30,
					resolution_base: RESOLUTION_1080P,
					compression: "Web",
					custom_bpp: null,
				},
				(msg) =>
					setState(
						reconcile({
							type: "preparing",
							rendered: msg.renderedCount,
							total: msg.totalFrames,
						}),
					),
			);

			setState(reconcile({ type: "uploading", progress: 0 }));

			const channel = new Channel<UploadProgress>((progress) => {
				setState(
					reconcile({
						type: "uploading",
						progress: Math.round(progress.progress * 100),
					}),
				);
			});

			return await commands.youtubeUploadRecording(projectPath, channel);
		},
		onSuccess: (result) => {
			setState(reconcile({ type: "done", url: result.url }));
		},
		onError: (error) => {
			setState(reconcile({ type: "error", message: formatError(error) }));
		},
	}));

	const dismiss = () => {
		setState(reconcile({ type: "idle" }));
		setCopied(false);
		upload.reset();
	};

	const copyLink = () => {
		if (state.type !== "done") return;
		navigator.clipboard.writeText(state.url);
		setCopied(true);
		toast.success("YouTube link copied");
	};

	return (
		<Show when={status()?.connected}>
			<Tooltip content="Render and upload this recording to YouTube as unlisted">
				<Button
					variant="gray"
					class="flex gap-1.5 justify-center items-center h-[40px]"
					disabled={upload.isPending}
					onClick={() => upload.mutate()}
				>
					<Show
						when={upload.isPending}
						fallback={<YouTubeGlyph class="size-4" />}
					>
						<IconLucideLoaderCircle class="animate-spin size-4" />
					</Show>
					<span class="text-xs">Upload to YouTube</span>
				</Button>
			</Tooltip>

			<Dialog.Root open={state.type !== "idle"}>
				<DialogContent
					title={
						state.type === "done"
							? "Upload complete"
							: state.type === "error"
								? "Upload failed"
								: "Uploading to YouTube"
					}
					confirm={
						<Switch>
							<Match when={state.type === "done"}>
								<Button variant="primary" onClick={dismiss}>
									Done
								</Button>
							</Match>
							<Match when={state.type === "error"}>
								<Button variant="gray" onClick={dismiss}>
									Close
								</Button>
							</Match>
							<Match when={inFlight()}>
								<Button variant="gray" disabled>
									Please wait…
								</Button>
							</Match>
						</Switch>
					}
					class="text-gray-12"
				>
					<div class="py-2 space-y-4">
						<Switch>
							<Match when={state.type === "error"}>
								<p class="text-sm leading-relaxed text-red-400">
									{errorMessage()}
								</p>
							</Match>
							<Match when={state.type === "done"}>
								<div class="space-y-3">
									<p class="text-sm text-gray-11">
										Your unlisted video is on YouTube. The link is copied to
										your clipboard.
									</p>
									<div class="flex gap-2 items-center p-2 rounded-lg bg-gray-3">
										<a
											href={doneUrl()}
											target="_blank"
											rel="noreferrer"
											class="flex-1 text-xs truncate text-blue-9 hover:underline"
										>
											{doneUrl()}
										</a>
										<Button variant="gray" size="sm" onClick={copyLink}>
											{copied() ? "Copied" : "Copy"}
										</Button>
									</div>
								</div>
							</Match>
							<Match when={inFlight()}>
								<div class="space-y-3">
									<div class="overflow-hidden w-full h-2 rounded-full bg-gray-3">
										<div
											class="h-full rounded-full transition-[width] duration-150 bg-blue-9"
											style={{ width: `${percent()}%` }}
										/>
									</div>
									<p class="text-xs text-gray-11">
										{state.type === "preparing"
											? `Preparing video… ${percent()}%`
											: `Uploading to YouTube… ${percent()}%`}
									</p>
								</div>
							</Match>
						</Switch>
					</div>
				</DialogContent>
			</Dialog.Root>
		</Show>
	);
}

export default YouTubeUploadButton;
