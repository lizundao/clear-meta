export type FieldAction = "keep" | "remove" | "set";

export interface FieldRule {
  key: string;
  action: FieldAction;
  value?: string;
}

export interface ProcessResult {
  path: string;
  success: boolean;
  message: string;
  output_path?: string;
  backup_path?: string;
  before_count: number;
  after_count?: number;
  before_fields: MetadataField[];
  after_fields?: MetadataField[];
}

export type OutputMode = "in_place" | "copy";

export interface ApplyOptions {
  output_mode: OutputMode;
  output_dir?: string;
  rules: FieldRule[];
  clear_all?: boolean;
}

export type ClearOptions = ApplyOptions;

export interface SupportedFormats {
  pdf: string[];
  office: string[];
  image: string[];
}

export interface MetadataField {
  key: string;
  group: string;
  name: string;
  value: string;
  editable?: boolean;
}

export interface MetadataPreview {
  path: string;
  success: boolean;
  message: string;
  fields: MetadataField[];
}

export interface CompareData {
  before: MetadataField[];
  after: MetadataField[];
  beforeCount: number;
  afterCount: number;
}

export type PreviewMode = "compare" | "before" | "after" | "edit";

export interface ReportContext {
  module: string;
  outputMode: OutputMode;
  outputDir?: string;
  processedAt: string;
}

export interface FieldEditState {
  action: FieldAction;
  value: string;
}
