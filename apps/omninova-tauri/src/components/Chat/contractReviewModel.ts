export interface ContractReviewEngineCard {
  id: string;
  name: string;
  description: string;
  reviewFocus: string[];
  clauses: string[];
  riskPolicy: string;
  outputSchema: string[];
  recommended: boolean;
}

export interface PreparedContractReview {
  prompt: string;
  markdown: string;
  export: unknown;
  mode: "review" | "comparison";
  engine: ContractReviewEngineCard;
}
