import { Button } from "@cap/ui-solid";
import { createWritableMemo } from "@solid-primitives/memo";
import { useMutation } from "@tanstack/solid-query";
import { createResource, createSignal, For, Show } from "solid-js";
import { Input } from "~/routes/editor/ui";
import {
	getYouTubeAutoUpload,
	setYouTubeAutoUpload,
} from "~/utils/automations";
import {
	commands,
	type YouTubeChannel,
	type YouTubePrivacy,
} from "~/utils/tauri";
import {
	Section,
	SectionCard,
	SettingsPageContent,
	ToggleSettingItem,
} from "../Setting";
import { IntegrationConfigHeader } from "./config-header";

const CONSOLE_URL = "https://console.cloud.google.com/apis/credentials";

function formatError(error: unknown): string {
	if (error && typeof error === "object" && "type" in error) {
		const e = error as { type: string; message?: unknown };
		if (e.message) {
			if (typeof e.message === "string") return e.message;
			if (typeof e.message === "object" && e.message && "message" in e.message)
				return String((e.message as { message: unknown }).message);
		}
		return e.type;
	}
	return error instanceof Error ? error.message : "Something went wrong";
}

export default function YouTubeConfigPage() {
	console.log("[yt-config] component invoked");
	const [status, { mutate: setStatus }] = createResource(() =>
		commands.youtubeGetStatus(),
	);

	const [showSetup, setShowSetup] = createSignal(false);
	const [clientId, setClientId] = createWritableMemo(() => "");
	const [clientSecret, setClientSecret] = createWritableMemo(() => "");

	const connected = () => status()?.connected ?? false;
	const hasCredentials = () => status()?.hasCredentials ?? false;

	const saveCredentials = useMutation(() => ({
		mutationFn: async () => {
			const next = await commands.youtubeSetCredentials(
				clientId(),
				clientSecret(),
			);
			setStatus(next);
		},
		onError: (error) => commands.globalMessageDialog(formatError(error)),
	}));

	const connect = useMutation(() => ({
		mutationFn: async () => {
			const next = await commands.youtubeConnect();
			setStatus(next);
			await refetchChannels();
		},
		onError: (error) => commands.globalMessageDialog(formatError(error)),
	}));

	const disconnect = useMutation(() => ({
		mutationFn: async () => {
			const next = await commands.youtubeDisconnect();
			setStatus(next);
		},
		onError: (error) => commands.globalMessageDialog(formatError(error)),
	}));

	const [channels, { refetch: refetchChannels }] = createResource(
		() => (connected() ? "connected" : null),
		async () => {
			try {
				return await commands.youtubeListChannels();
			} catch (error) {
				console.error("Failed to list YouTube channels", error);
				return [] as YouTubeChannel[];
			}
		},
	);

	const setChannel = useMutation(() => ({
		mutationFn: async (channel: YouTubeChannel) => {
			const next = await commands.youtubeSetChannel(channel.id, channel.title);
			setStatus(next);
		},
		onError: (error) => commands.globalMessageDialog(formatError(error)),
	}));

	const [autoUpload, { mutate: setAutoUploadState }] = createResource(
		() => (connected() ? "connected" : null),
		() => getYouTubeAutoUpload(),
	);

	const changePrivacy = useMutation(() => ({
		mutationFn: async (privacy: YouTubePrivacy) => {
			const next = await commands.youtubeSetPreferences(
				autoUpload() ?? false,
				privacy,
			);
			setStatus(next);
			// Keep the managed auto-upload rules in sync with the chosen privacy.
			if (autoUpload()) await setYouTubeAutoUpload(true, privacy);
		},
		onError: (error) => commands.globalMessageDialog(formatError(error)),
	}));

	const toggleAutoUpload = useMutation(() => ({
		mutationFn: async (value: boolean) => {
			await setYouTubeAutoUpload(value, status()?.defaultPrivacy ?? "unlisted");
			const next = await commands.youtubeSetPreferences(
				value,
				status()?.defaultPrivacy ?? "unlisted",
			);
			setStatus(next);
			setAutoUploadState(value);
		},
		onError: (error) => commands.globalMessageDialog(formatError(error)),
	}));

	const busy = () =>
		status.loading ||
		saveCredentials.isPending ||
		connect.isPending ||
		disconnect.isPending;

	return (
		<div class="cap-settings-page flex flex-col h-full custom-scroll">
			<div style="background:#c00;color:#fff;padding:8px;font-size:14px">
				YT-CONFIG-DEBUG: page rendered
			</div>
			<SettingsPageContent>
				<IntegrationConfigHeader title="YouTube" />
				<div class="space-y-6">
					<Section
						title="Google API credentials"
						description="Cap uploads with your own Google OAuth client, so it stays independent of any hosted service. Create a project once and paste the credentials below."
					>
						<SectionCard padded class="space-y-4">
							<button
								type="button"
								class="text-xs underline text-gray-11 hover:text-gray-12 w-fit"
								onClick={() => setShowSetup((v) => !v)}
							>
								{showSetup() ? "Hide setup guide" : "How do I get these?"}
							</button>
							<Show when={showSetup()}>
								<ol class="pl-4 space-y-1.5 text-xs list-decimal text-gray-11 leading-relaxed">
									<li>
										Open the{" "}
										<button
											type="button"
											class="underline text-gray-12"
											onClick={() => commands.openExternalLink(CONSOLE_URL)}
										>
											Google Cloud Console
										</button>{" "}
										and create a project.
									</li>
									<li>Enable the "YouTube Data API v3" for that project.</li>
									<li>
										Configure the OAuth consent screen (External). To avoid
										re-authenticating every 7 days, publish it to Production —
										you can accept the "unverified app" screen for your own
										project.
									</li>
									<li>
										Create an OAuth client ID with application type{" "}
										<span class="text-gray-12">Desktop app</span>.
									</li>
									<li>Copy the Client ID and secret into the fields below.</li>
								</ol>
							</Show>

							<div class="space-y-2">
								<label class="text-[13px] text-gray-12">Client ID</label>
								<Input
									class="bg-gray-3!"
									value={clientId()}
									placeholder="xxxxxxxx.apps.googleusercontent.com"
									autocomplete="off"
									autocapitalize="off"
									autocorrect="off"
									spellcheck={false}
									onInput={(
										e: InputEvent & { currentTarget: HTMLInputElement },
									) => setClientId(e.currentTarget.value)}
								/>
							</div>
							<div class="space-y-2">
								<label class="text-[13px] text-gray-12">Client secret</label>
								<Input
									class="bg-gray-3!"
									type="password"
									value={clientSecret()}
									placeholder="GOCSPX-..."
									autocomplete="off"
									autocapitalize="off"
									autocorrect="off"
									spellcheck={false}
									onInput={(
										e: InputEvent & { currentTarget: HTMLInputElement },
									) => setClientSecret(e.currentTarget.value)}
								/>
							</div>
							<div class="flex justify-between items-center">
								<Show when={hasCredentials()}>
									<p class="text-xs text-gray-10">Credentials saved</p>
								</Show>
								<Button
									class="ml-auto"
									variant="primary"
									disabled={
										busy() || (!clientId().trim() && !clientSecret().trim())
									}
									onClick={() => saveCredentials.mutate()}
								>
									{saveCredentials.isPending ? "Saving..." : "Save credentials"}
								</Button>
							</div>
						</SectionCard>
					</Section>

					<Section
						title="Connection"
						description="Sign in to YouTube and choose which channel finished recordings upload to."
					>
						<SectionCard padded class="space-y-4">
							<div class="flex justify-between items-center gap-4">
								<div class="flex flex-col gap-0.5 min-w-0">
									<p class="text-[13px] text-gray-12">
										{connected()
											? (status()?.channelTitle ?? "Connected")
											: "Not connected"}
									</p>
									<p class="text-xs leading-snug text-gray-10">
										{connected()
											? "Ready to upload unlisted videos"
											: "Connect your Google account to continue"}
									</p>
								</div>
								<Show
									when={connected()}
									fallback={
										<Button
											variant="primary"
											disabled={busy() || !hasCredentials()}
											onClick={() => connect.mutate()}
										>
											{connect.isPending ? "Waiting..." : "Connect YouTube"}
										</Button>
									}
								>
									<Button
										variant="destructive"
										disabled={busy()}
										onClick={() => disconnect.mutate()}
									>
										{disconnect.isPending ? "Disconnecting..." : "Disconnect"}
									</Button>
								</Show>
							</div>

							<Show when={connected()}>
								<div class="pt-4 space-y-2 border-t border-gray-3">
									<label class="text-[13px] text-gray-12">Channel</label>
									<div class="relative">
										<select
											value={status()?.channelId ?? ""}
											disabled={setChannel.isPending}
											onChange={(e) => {
												const channel = channels()?.find(
													(c) => c.id === e.currentTarget.value,
												);
												if (channel) setChannel.mutate(channel);
											}}
											class="px-3 py-2 pr-10 w-full rounded-lg border border-transparent transition-all duration-200 appearance-none outline-hidden bg-gray-3 focus:border-gray-8 text-gray-12"
										>
											<Show when={!status()?.channelId}>
												<option value="">Select a channel...</option>
											</Show>
											<For each={channels() ?? []}>
												{(channel) => (
													<option value={channel.id}>{channel.title}</option>
												)}
											</For>
										</select>
									</div>
									<Show when={(channels()?.length ?? 0) === 0}>
										<button
											type="button"
											class="text-xs underline text-gray-11 hover:text-gray-12"
											onClick={() => refetchChannels()}
										>
											Refresh channels
										</button>
									</Show>
								</div>
							</Show>
						</SectionCard>
					</Section>

					<Show when={connected()}>
						<Section
							title="Upload preferences"
							description="Control how recordings are sent to YouTube."
						>
							<SectionCard padded class="space-y-4">
								<ToggleSettingItem
									label="Auto-upload finished recordings"
									description="Upload every finished studio and instant recording to your selected channel automatically."
									value={autoUpload() ?? false}
									onChange={(value) => toggleAutoUpload.mutate(value)}
								/>
								<div class="space-y-2">
									<label class="text-[13px] text-gray-12">
										Default privacy
									</label>
									<div class="relative">
										<select
											value={status()?.defaultPrivacy ?? "unlisted"}
											disabled={
												changePrivacy.isPending || toggleAutoUpload.isPending
											}
											onChange={(e) =>
												changePrivacy.mutate(
													e.currentTarget.value as YouTubePrivacy,
												)
											}
											class="px-3 py-2 pr-10 w-full rounded-lg border border-transparent transition-all duration-200 appearance-none outline-hidden bg-gray-3 focus:border-gray-8 text-gray-12"
										>
											<option value="unlisted">Unlisted</option>
											<option value="private">Private</option>
											<option value="public">Public</option>
										</select>
									</div>
								</div>
							</SectionCard>
						</Section>
					</Show>
				</div>
			</SettingsPageContent>
		</div>
	);
}
