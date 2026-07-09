import { Button } from "@cap/ui-solid";
import { createMutation } from "@tanstack/solid-query";
import { Channel } from "@tauri-apps/api/core";
import { createResource, Show } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import toast from "solid-toast";
import Tooltip from "~/components/Tooltip";
import { exportVideo } from "~/utils/export";
import { commands, type UploadProgress } from "~/utils/tauri";
import { useEditorContext } from "./context";

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

function YouTubeUploadButton() {
	const { editorInstance, meta } = useEditorContext();
	const projectPath = editorInstance.path;

	const [status, { refetch }] = createResource(() =>
		commands.youtubeGetStatus(),
	);

	const [state, setState] = createStore<
		| { type: "idle" }
		| { type: "rendering"; rendered: number; total: number }
		| { type: "uploading"; progress: number }
	>({ type: "idle" });

	const alreadyUploaded = () => !!meta().youtube;

	const copyLink = () => {
		const url = meta().youtube?.url;
		if (!url) return;
		navigator.clipboard.writeText(url);
		toast.success("YouTube link copied");
	};

	const upload = createMutation(() => ({
		mutationFn: async () => {
			if (!navigator.onLine)
				throw new Error("You appear to be offline. Check your connection.");

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
							type: "rendering",
							rendered: msg.renderedCount,
							total: msg.totalFrames,
						}),
					),
			);

			setState({ type: "uploading", progress: 0 });

			const channel = new Channel<UploadProgress>((progress) => {
				setState({
					type: "uploading",
					progress: Math.round(progress.progress * 100),
				});
			});

			return await commands.youtubeUploadRecording(projectPath, channel);
		},
		onSuccess: async () => {
			await refetch();
			toast.success("Uploaded to YouTube — link copied to clipboard");
		},
		onError: (error) => {
			toast.error(formatError(error));
		},
		onSettled: () => {
			setState({ type: "idle" });
		},
	}));

	const label = () => {
		if (state.type === "rendering")
			return `Rendering ${state.total ? Math.round((state.rendered / state.total) * 100) : 0}%`;
		if (state.type === "uploading") return `Uploading ${state.progress}%`;
		return alreadyUploaded() ? "Copy YouTube link" : "Upload to YouTube";
	};

	return (
		<Show when={status()?.connected}>
			<Tooltip
				content={
					alreadyUploaded()
						? "Copy the unlisted YouTube link"
						: "Render and upload this recording to YouTube as unlisted"
				}
			>
				<Button
					variant="gray"
					class="flex gap-1.5 justify-center items-center h-[40px]"
					disabled={upload.isPending}
					onClick={() => {
						if (alreadyUploaded() && !upload.isPending) {
							copyLink();
							return;
						}
						upload.mutate();
					}}
				>
					<Show
						when={upload.isPending}
						fallback={<YouTubeGlyph class="size-4" />}
					>
						<IconLucideLoaderCircle class="animate-spin size-4" />
					</Show>
					<span class="text-xs">{label()}</span>
				</Button>
			</Tooltip>
		</Show>
	);
}

export default YouTubeUploadButton;
