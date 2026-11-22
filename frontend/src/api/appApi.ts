import type {
  AboutDto,
  AiSettingsDto,
  AiModelInput,
  AppInfoDto,
  AttributionDto,
  BackupDto,
  BackupInput,
  DeleteBackupInput,
  BulkActionInput,
  BulkPreviewDto,
  BulkPreviewInput,
  BulkResultDto,
  CardActionInput,
  CardActionResultDto,
  CardPageDto,
  CollectionChangedEventDto,
  CommitImportInput,
  CommitDeckImportInput,
  CompleteOnboardingInput,
  CreateDeckInput,
  DashboardDto,
  DeckOverviewDto,
  DiagnosticsReportDto,
  ExportDeckInput,
  DeleteDeckInput,
  DeleteDeckResultDto,
  DeleteNoteInput,
  DeleteNoteResultDto,
  DuplicateCheckDto,
  DuplicateCheckInput,
  EditorDataDto,
  FileHandleDto,
  GenerateAssociationInput,
  Id,
  ImportInput,
  ImportPreviewDto,
  JobDto,
  JobEventDto,
  NoteContentDto,
  NoteDetailsDto,
  OnboardingStateDto,
  PreviewDeckImportInput,
  DeckImportPreviewDto,
  PickFileInput,
  PresetDto,
  GetOptimizerInput,
  GetGrammarInput,
  GrammarDto,
  OptimizerDto,
  OptimizerRunActionInput,
  StartOptimizerInput,
  PreviewInput,
  RateCardInput,
  RequestPermissionInput,
  ResetInput,
  ResetPreviewDto,
  ResetPreviewInput,
  RatingPreviewDto,
  ResolveWordLinkInput,
  AddWordLinkInput,
  RestoreInput,
  RestorePreviewDto,
  ReviewResultDto,
  SaveNoteInput,
  SavePresetInput,
  SearchInput,
  SearchWordsInput,
  SessionSummaryDto,
  SetAiApiKeyInput,
  SetFavoriteAssociationInput,
  ShareFileInput,
  ShareResultDto,
  SettingsDto,
  StartStudyInput,
  StatsDto,
  StatsInput,
  StudySessionDto,
  UndoInput,
  Unsubscribe,
  UpdateDeckInput,
  UpdateNoteContentInput,
  UpdateSettingsInput,
  UpdateStudySessionInput,
  AssociationDto,
  WordLinkDto,
  WordMatchDto,
  ValidateRestoreInput,
} from './types'

/**
 * The only gateway from screens to the backend. Implemented by `MockAppApi` (demo, Phases 2–6)
 * and the Tauri adapter (production). Rejections are always `AppError` carrying an `ErrorDto`.
 */
export interface AppApi {
  // app
  getAppInfo(): Promise<AppInfoDto>
  getAttributions(): Promise<AttributionDto[]>
  /** Versions for the About screen: app, contract, schema, FSRS, bundled content. */
  getAboutInfo(): Promise<AboutDto>
  /** Recent log lines, redacted by the backend before they cross IPC. */
  getDiagnosticsReport(): Promise<DiagnosticsReportDto>

  // dashboard / decks
  getDashboard(): Promise<DashboardDto>
  getDeck(deckId: Id): Promise<DeckOverviewDto>
  createDeck(input: CreateDeckInput): Promise<DeckOverviewDto>
  updateDeck(input: UpdateDeckInput): Promise<DeckOverviewDto>
  deleteDeck(input: DeleteDeckInput): Promise<DeleteDeckResultDto>
  listPresets(): Promise<PresetDto[]>
  /** Creates (also clones) or updates a preset; a change applies to every dependent deck. */
  savePreset(input: SavePresetInput): Promise<PresetDto>
  /** Optimizer state of a preset: usable history, running job, earlier runs (Phase 18). */
  getOptimizer(input: GetOptimizerInput): Promise<OptimizerDto>
  /** Job `optimize`: trains on the device; the preset stays unchanged until a run is applied. */
  startOptimizer(input: StartOptimizerInput): Promise<JobDto>
  /** Sets the preset weights to the run's proposal; no card is rescheduled. */
  applyOptimizerRun(input: OptimizerRunActionInput): Promise<PresetDto>
  /** Restores the weights the preset had before the applied run. */
  revertOptimizerRun(input: OptimizerRunActionInput): Promise<PresetDto>

  // grammar
  /** Grammar tips grouped by deck chapter: decks with tips and the chapters of one deck (1.6). */
  getGrammar(input: GetGrammarInput): Promise<GrammarDto>

  // study
  startStudy(input: StartStudyInput): Promise<StudySessionDto>
  resumeStudy(sessionId: Id): Promise<StudySessionDto>
  /** Pause, resume or end a session; ending early keeps every committed rating. */
  updateStudySession(input: UpdateStudySessionInput): Promise<StudySessionDto>
  getRatingPreview(input: PreviewInput): Promise<RatingPreviewDto>
  rateCard(input: RateCardInput): Promise<ReviewResultDto>
  undoReview(input: UndoInput): Promise<ReviewResultDto>
  applyCardAction(input: CardActionInput): Promise<CardActionResultDto>
  getSessionSummary(sessionId: Id): Promise<SessionSummaryDto>

  // details / editor
  getNoteDetails(cardId: Id): Promise<NoteDetailsDto>
  getNoteContent(noteId: Id): Promise<NoteContentDto>
  getEditorData(): Promise<EditorDataDto>
  checkDuplicate(input: DuplicateCheckInput): Promise<DuplicateCheckDto>
  saveNote(input: SaveNoteInput): Promise<NoteDetailsDto>
  /** Deletes the note with all its cards and their history. */
  deleteNote(input: DeleteNoteInput): Promise<DeleteNoteResultDto>
  updateNoteContent(input: UpdateNoteContentInput): Promise<NoteDetailsDto>
  setFavoriteAssociation(input: SetFavoriteAssociationInput): Promise<AssociationDto[]>
  resolveWordLink(input: ResolveWordLinkInput): Promise<WordLinkDto>
  /** Links one more phrase of a sentence to a word note; resolves to every link of the sentence. */
  addWordLink(input: AddWordLinkInput): Promise<WordLinkDto[]>
  /** Word notes matching a selected phrase: exact first, then normalized; empty when nothing matches. */
  searchWords(input: SearchWordsInput): Promise<WordMatchDto[]>
  generateAssociation(input: GenerateAssociationInput): Promise<JobDto>

  // browser
  searchCards(input: SearchInput): Promise<CardPageDto>
  previewBulkAction(input: BulkPreviewInput): Promise<BulkPreviewDto>
  applyBulkAction(input: BulkActionInput): Promise<BulkResultDto>

  // statistics / settings / onboarding
  getStats(input: StatsInput): Promise<StatsDto>
  getSettings(): Promise<SettingsDto>
  updateSettings(input: UpdateSettingsInput): Promise<SettingsDto>
  /** Stores or removes the BYOK key in the SecretStore; only `hasApiKey` comes back. */
  setAiApiKey(input: SetAiApiKeyInput): Promise<AiSettingsDto>
  /** Starts or resumes downloading an on-device model; progress arrives as settings changes. */
  downloadAiModel(input: AiModelInput): Promise<AiSettingsDto>
  /** Cancels a download and removes the model from the device. */
  deleteAiModel(input: AiModelInput): Promise<AiSettingsDto>
  /** Asks the OS for notification permission; a refusal is a state, not a failure. */
  requestNotificationPermission(input: RequestPermissionInput): Promise<SettingsDto>
  getOnboardingState(): Promise<OnboardingStateDto>
  completeOnboarding(input: CompleteOnboardingInput): Promise<JobDto>
  previewReset(input: ResetPreviewInput): Promise<ResetPreviewDto>
  /** Resets progress or deletes everything; requires a fresh safety backup or the confirmation phrase. */
  resetCollection(input: ResetInput): Promise<JobDto>

  // transfer / backup
  pickFile(input: PickFileInput): Promise<FileHandleDto | null>
  previewImport(input: ImportInput): Promise<ImportPreviewDto>
  commitImport(input: CommitImportInput): Promise<JobDto>
  /** Dry run of a deck package (e.g. converted from Anki by apkg-convert); nothing is written. */
  previewDeckImport(input: PreviewDeckImportInput): Promise<DeckImportPreviewDto>
  /** Imports the previewed package with its progress and review history (job `import`). */
  commitDeckImport(input: CommitDeckImportInput): Promise<JobDto>
  exportDeck(input: ExportDeckInput): Promise<JobDto>
  /** Opens the platform share sheet for a backup or a finished export. */
  shareFile(input: ShareFileInput): Promise<ShareResultDto>
  listBackups(): Promise<BackupDto[]>
  createBackup(input: BackupInput): Promise<JobDto>
  /** Deletes one backup file; returns the backups that remain, newest first. */
  deleteBackup(input: DeleteBackupInput): Promise<BackupDto[]>
  validateRestore(input: ValidateRestoreInput): Promise<RestorePreviewDto>
  /** Makes a safety copy of the current collection first; a failed restore leaves it untouched. */
  restoreBackup(input: RestoreInput): Promise<JobDto>

  // jobs / events
  getJobStatus(jobId: Id): Promise<JobDto>
  cancelJob(jobId: Id): Promise<JobDto>
  /** Synchronous unsubscribe; async registration is handled inside the adapter. */
  onJobEvent(listener: (event: JobEventDto) => void): Unsubscribe
  onCollectionChanged(listener: (event: CollectionChangedEventDto) => void): Unsubscribe
}

export type AppApiOperation = keyof AppApi
